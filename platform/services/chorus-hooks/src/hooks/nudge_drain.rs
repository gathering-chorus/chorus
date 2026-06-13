//! #3218 — nudge drain: the reliable receiver-side primitive.
//!
//! Reads pending nudges for a role from `messages.db` DIRECTLY (local rusqlite)
//! and atomically marks them delivered in the SAME transaction (drain-once).
//! Wired into PreToolUse (every tool call) AND the Stop hook (turn boundary) so
//! a role cannot run a string of tool calls — or end a turn / go idle — blind to
//! a pending peer nudge. This is the structural fix for the hold-and-wait
//! deadlock: two long-running stream-aligned agents that are each other's
//! required reviewer no longer go blind on contact (the queue isn't theirs to
//! reorder, and Jeff's gravity can't bump a peer out of it).
//!
//! Why `messages.db` and not the `chorus.log` fold (`nudge_poll`): the fold is
//! window-bounded by design (its own `respects_window_bound` test asserts an
//! aged nudge returns empty). That's fine for a best-effort turn-boundary
//! augment, but for #3218's guarantee — "can't go idle with a pending nudge" —
//! a window-miss is a DROPPED nudge, exactly the bug being fixed. `messages.db`
//! indexed by `delivery_status='pending'` has no window: every pending nudge is
//! found regardless of age, and atomic mark-delivered gives true drain-once with
//! no re-injection race.
//!
//! NEVER calls the messaging API (:3475) — that wedges under load; this is a
//! THIRD LOCAL reader of the DEC-107 persist store. chorus-messaging owns
//! writes; persist + osascript still fire every time. Additive, not competing.
//!
//! Delivery semantics (Wren / DEC-107 lane):
//!   FIFO        — oldest-first by created_at, id as tiebreaker (arrival order).
//!   drain-once  — select + mark-delivered atomic in one txn; a 2nd call is empty.
//!   never-block — the handler emits the drained set as additionalContext at
//!                 exit 0; a drain failure returns empty and NEVER refuses a tool.
//!   always-on   — fires on every PreToolUse incl. mid-Jeff-conversation; the
//!                 wiring must not narrow it to skip Jeff turns.
//!
//! Operational guards (#3218, Kade review):
//!   headless    — inside the act/werk.yml runner (GITHUB_ACTIONS / ACT /
//!                 CHORUS_HEADLESS) the drain is a HARD no-op: no terminal to
//!                 inject into, and a block/inject would stall the pipeline
//!                 team-wide (the #3318 AC6 lesson — scope in, don't land-scope).
//!   O(1) idle   — one process-cached connection (no per-tool-call open); the
//!                 empty-queue hot path is lock + one indexed query (~0 cost).
//!   backlog cap — at most DRAIN_CAP per call, oldest-first, marking only those
//!                 delivered: a delivery-outage backlog drains in batches instead
//!                 of flooding context, and per-call cost stays bounded.
//!   no deadlock — the drain reads the agent's OWN queue and either injects
//!                 (non-blocking) or makes it address its OWN pending at Stop; no
//!                 agent ever waits on another's drain (that was the *gather*
//!                 ceremony, #3384). drain-once bounds the Stop-block to one
//!                 continuation. So A-blocks-on-B-blocks-on-A cannot arise here.

use rusqlite::Connection;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Max nudges drained per call (#3218, Kade review). The drain takes the oldest
/// N and marks ONLY those delivered, so a large backlog (e.g. a delivery-outage
/// recovery) drains N-per-tool-call oldest-first instead of flooding the agent's
/// context in one shot — and per-call cost stays bounded to N rows.
///
/// Note (Kade): a one-time outage backlog catches up at N/Stop. But under
/// SUSTAINED delivery-down where inflow exceeds N per Stop, the backlog holds
/// steady rather than shrinks — that is CORRECT behavior (the fix is to restore
/// delivery, not to drain harder). A held backlog here is not a drain bug.
const DRAIN_CAP: usize = 20;

/// One pending nudge drained from messages.db, in delivery (FIFO) order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingNudge {
    pub id: i64,
    pub from: String,
    pub content: String,
    pub trace_id: Option<String>,
}

/// Atomically drain the pending nudges addressed to `role`: select every
/// `type='nudge'` row with `delivery_status='pending'`, oldest-first (FIFO), and
/// mark them delivered in the SAME transaction. Returns the drained set in
/// delivery order; a second call returns empty (drain-once).
///
/// Never blocks and never panics: any DB error yields an empty result, because a
/// drain failure must never refuse a tool call (inject-as-context, not a gate).
pub fn drain_pending(conn: &mut Connection, role: &str) -> Vec<PendingNudge> {
    drain_pending_inner(conn, role).unwrap_or_default()
}

fn drain_pending_inner(conn: &mut Connection, role: &str) -> rusqlite::Result<Vec<PendingNudge>> {
    let tx = conn.transaction()?;
    let drained: Vec<PendingNudge> = {
        let mut stmt = tx.prepare(
            r#"SELECT id, "from", content, trace_id
               FROM messages
               WHERE "to" = ?1
                 AND type = 'nudge'
                 AND delivery_status = 'pending'
                 AND "from" != "to"
               ORDER BY created_at ASC, id ASC
               LIMIT ?2"#,
        )?;
        let rows = stmt.query_map(rusqlite::params![role, DRAIN_CAP as i64], |r| {
            Ok(PendingNudge {
                id: r.get(0)?,
                from: r.get(1)?,
                content: r.get(2)?,
                trace_id: r.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    if !drained.is_empty() {
        // Mark delivered in the same txn — drain-once, atomic with the read.
        // ids come straight from the DB (i64), so the IN-list is injection-safe.
        let id_list = drained
            .iter()
            .map(|n| n.id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        tx.execute(
            &format!(
                "UPDATE messages SET delivery_status='delivered', \
                 delivered_at=datetime('now') WHERE id IN ({id_list})"
            ),
            [],
        )?;
    }

    tx.commit()?;
    Ok(drained)
}

/// Format drained nudges into a markdown block for additionalContext injection.
/// Returns `None` when there's nothing to inject (caller emits no block, exit 0).
/// Oldest-first order is preserved from `drain_pending` — do not re-sort.
pub fn format_drain_block(role: &str, nudges: &[PendingNudge]) -> Option<String> {
    if nudges.is_empty() {
        return None;
    }
    let mut out = format!(
        "\n## Pending nudges ({} for {} — FIFO, act on the oldest first)\n",
        nudges.len(),
        role
    );
    for n in nudges {
        let tid = n.trace_id.as_deref().unwrap_or("-");
        out.push_str(&format!("- from {} ({}): {}\n", n.from, tid, n.content));
    }
    // #3218 (Kade): when a full cap was drained there may be more pending — say so,
    // so a backlog reads as "draining in batches", not "this is all there is".
    if nudges.len() >= DRAIN_CAP {
        out.push_str(&format!(
            "- (oldest {DRAIN_CAP} shown — cap; more pending drain on your next tool calls)\n"
        ));
    }
    Some(out)
}

/// A single cached connection to messages.db, opened once for the life of the
/// hook process. The empty-queue hot path (#3218, Kade) is then just lock + one
/// indexed query (no per-tool-call open), so a tool-heavy session pays ~0 when
/// nothing is pending. rusqlite Connection isn't Sync, so it lives behind a
/// Mutex; drains are sub-ms, so the serialization is negligible.
static DRAIN_CONN: OnceLock<Mutex<Connection>> = OnceLock::new();

fn cached_conn(db_path: &str) -> Option<&'static Mutex<Connection>> {
    if let Some(c) = DRAIN_CONN.get() {
        return Some(c);
    }
    // Open lazily; on failure return None WITHOUT caching, so a transient open
    // error doesn't wedge the hot path to a permanent no-op (never blocks).
    let conn = Connection::open(db_path).ok()?;
    let _ = conn.busy_timeout(Duration::from_millis(50));
    Some(DRAIN_CONN.get_or_init(|| Mutex::new(conn)))
}

/// Drain `role`'s pending nudges from the live messages.db and return the
/// formatted additionalContext block — or `None` when nothing is pending, the DB
/// is unreachable, or we're running headless. NEVER blocks: any error yields
/// `None` and the tool call proceeds.
pub fn drain_block_from_db(role: &str, db_path: &str) -> Option<String> {
    // #3218 (Kade, the #3318 lesson): inside the act/werk.yml runner there is no
    // role terminal — the drain MUST be a hard no-op or it stalls/fails the demo
    // and land jobs team-wide. Scope it in from the start, never land-then-scope.
    if std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("ACT").is_ok()
        || std::env::var("CHORUS_HEADLESS").is_ok()
    {
        return None;
    }
    let conn_lock = cached_conn(db_path)?;
    let mut conn = conn_lock.lock().ok()?;
    let drained = drain_pending(&mut conn, role);
    format_drain_block(role, &drained)
}

/// PEEK the pending nudges for `role` WITHOUT marking them delivered — returns
/// the formatted block if any are pending. This backs the #3218 respond-first
/// gate: the gate must keep blocking the agent's tool calls until it actually
/// RESPONDS, so the read must NOT consume (consuming would clear the block after
/// one tool call and re-enable defer). The queue is cleared separately, only
/// when the agent sends its reply (via `drain_block_from_db` on the reply path).
/// Headless = no-op, same guard as the drain.
pub fn peek_pending_block(role: &str, db_path: &str) -> Option<String> {
    if std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("ACT").is_ok()
        || std::env::var("CHORUS_HEADLESS").is_ok()
    {
        return None;
    }
    let conn_lock = cached_conn(db_path)?;
    let conn = conn_lock.lock().ok()?;
    let pending = peek_pending(&conn, role);
    format_drain_block(role, &pending)
}

/// Read the pending nudges for `role` without marking them delivered (the
/// non-consuming counterpart to `drain_pending`). Same scope + cap + FIFO order.
fn peek_pending(conn: &Connection, role: &str) -> Vec<PendingNudge> {
    peek_pending_inner(conn, role).unwrap_or_default()
}

fn peek_pending_inner(conn: &Connection, role: &str) -> rusqlite::Result<Vec<PendingNudge>> {
    let mut stmt = conn.prepare(
        r#"SELECT id, "from", content, trace_id
           FROM messages
           WHERE "to" = ?1
             AND type = 'nudge'
             AND delivery_status = 'pending'
             AND "from" != "to"
           ORDER BY created_at ASC, id ASC
           LIMIT ?2"#,
    )?;
    let rows = stmt.query_map(rusqlite::params![role, DRAIN_CAP as i64], |r| {
        Ok(PendingNudge {
            id: r.get(0)?,
            from: r.get(1)?,
            content: r.get(2)?,
            trace_id: r.get(3)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// A minimal messages table matching the live schema columns the drain reads
    /// (platform/pulse/messages.db). Tests insert explicit created_at values so
    /// FIFO order is deterministic (datetime('now') is only second-resolution).
    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(
            r#"CREATE TABLE messages (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 type TEXT NOT NULL,
                 "from" TEXT NOT NULL,
                 "to" TEXT NOT NULL,
                 content TEXT NOT NULL,
                 acknowledged INTEGER DEFAULT 0,
                 created_at TEXT,
                 delivery_status TEXT NOT NULL DEFAULT 'pending',
                 delivered_at TEXT,
                 trace_id TEXT
               );"#,
        )
        .expect("create messages table");
        conn
    }

    fn insert(conn: &Connection, typ: &str, from: &str, to: &str, content: &str, created_at: &str) {
        conn.execute(
            r#"INSERT INTO messages (type, "from", "to", content, created_at, delivery_status, trace_id)
               VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6)"#,
            rusqlite::params![typ, from, to, content, created_at, format!("ntr-{content}")],
        )
        .expect("insert message");
    }

    #[test]
    fn drains_pending_fifo_oldest_first() {
        let mut conn = setup_db();
        insert(&conn, "nudge", "wren", "silas", "first", "2026-06-13 10:00:01");
        insert(&conn, "nudge", "kade", "silas", "second", "2026-06-13 10:00:02");
        insert(&conn, "nudge", "wren", "silas", "third", "2026-06-13 10:00:03");

        let drained = drain_pending(&mut conn, "silas");
        let order: Vec<&str> = drained.iter().map(|n| n.content.as_str()).collect();
        assert_eq!(order, vec!["first", "second", "third"], "FIFO oldest-first");
    }

    #[test]
    fn drain_once_second_call_is_empty() {
        let mut conn = setup_db();
        insert(&conn, "nudge", "wren", "silas", "only", "2026-06-13 10:00:01");

        let first = drain_pending(&mut conn, "silas");
        assert_eq!(first.len(), 1, "first drain returns the pending nudge");
        let second = drain_pending(&mut conn, "silas");
        assert!(second.is_empty(), "drain-once: second call is empty (marked delivered)");
    }

    #[test]
    fn peek_does_not_consume() {
        // The respond-first gate depends on this: peeking must NOT mark delivered,
        // or the block would clear after one tool call and re-enable defer (#3218).
        let mut conn = setup_db();
        insert(&conn, "nudge", "wren", "silas", "still-here", "2026-06-13 10:00:01");

        assert_eq!(peek_pending(&conn, "silas").len(), 1, "peek sees the pending nudge");
        assert_eq!(peek_pending(&conn, "silas").len(), 1, "peek again — still there, peek must not consume");
        // Only a real drain (the reply path) clears it.
        assert_eq!(drain_pending(&mut conn, "silas").len(), 1, "drain consumes it");
        assert!(peek_pending(&conn, "silas").is_empty(), "after the reply-path drain, the block clears");
    }

    #[test]
    fn marks_delivered_atomically() {
        let mut conn = setup_db();
        insert(&conn, "nudge", "wren", "silas", "mark-me", "2026-06-13 10:00:01");

        drain_pending(&mut conn, "silas");
        let (status, delivered): (String, Option<String>) = conn
            .query_row(
                "SELECT delivery_status, delivered_at FROM messages WHERE content='mark-me'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("row exists");
        assert_eq!(status, "delivered", "row flipped to delivered");
        assert!(delivered.is_some(), "delivered_at stamped");
    }

    #[test]
    fn excludes_already_delivered() {
        let mut conn = setup_db();
        insert(&conn, "nudge", "wren", "silas", "pending-one", "2026-06-13 10:00:02");
        conn.execute(
            r#"INSERT INTO messages (type, "from", "to", content, created_at, delivery_status)
               VALUES ('nudge', 'kade', 'silas', 'already-gone', '2026-06-13 10:00:01', 'delivered')"#,
            [],
        )
        .unwrap();

        let drained = drain_pending(&mut conn, "silas");
        assert_eq!(drained.len(), 1, "only the pending nudge drains");
        assert_eq!(drained[0].content, "pending-one");
    }

    #[test]
    fn recipient_scoped() {
        let mut conn = setup_db();
        insert(&conn, "nudge", "wren", "kade", "for-kade", "2026-06-13 10:00:01");
        insert(&conn, "nudge", "wren", "silas", "for-silas", "2026-06-13 10:00:02");

        let drained = drain_pending(&mut conn, "silas");
        assert_eq!(drained.len(), 1, "only silas's nudge drains");
        assert_eq!(drained[0].content, "for-silas");
        // kade's nudge is untouched (still pending for a later kade drain)
        let kade = drain_pending(&mut conn, "kade");
        assert_eq!(kade.len(), 1, "kade's nudge was not consumed by silas's drain");
    }

    #[test]
    fn nudge_type_only() {
        let mut conn = setup_db();
        insert(&conn, "chat", "wren", "silas", "just-chatter", "2026-06-13 10:00:01");
        insert(&conn, "board-event", "system", "silas", "a-board-event", "2026-06-13 10:00:02");
        insert(&conn, "nudge", "wren", "silas", "real-nudge", "2026-06-13 10:00:03");

        let drained = drain_pending(&mut conn, "silas");
        assert_eq!(drained.len(), 1, "only type='nudge' drains; chat/board-event ignored");
        assert_eq!(drained[0].content, "real-nudge");
    }

    #[test]
    fn excludes_self_sent() {
        // A self-sent row (from == to) must never surface as a peer interruption,
        // even though it matches recipient+type+pending (Silas review, #3218).
        let mut conn = setup_db();
        insert(&conn, "nudge", "silas", "silas", "talking-to-myself", "2026-06-13 10:00:01");
        insert(&conn, "nudge", "wren", "silas", "real-peer-nudge", "2026-06-13 10:00:02");

        let drained = drain_pending(&mut conn, "silas");
        assert_eq!(drained.len(), 1, "self-sent row is not a peer nudge");
        assert_eq!(drained[0].content, "real-peer-nudge");
    }

    #[test]
    fn empty_when_none_pending_no_error() {
        let mut conn = setup_db();
        let drained = drain_pending(&mut conn, "silas");
        assert!(drained.is_empty(), "no pending → empty, no error, never blocks");
    }

    #[test]
    fn caps_drain_at_limit_and_catches_up() {
        // #3218 (Kade): a backlog must not flood — drain at most DRAIN_CAP per
        // call, oldest-first, marking only those delivered, and catch up over
        // subsequent calls.
        let mut conn = setup_db();
        for i in 0..(DRAIN_CAP + 5) {
            insert(&conn, "nudge", "wren", "silas", &format!("n{i}"), &format!("2026-06-13 10:00:{i:02}"));
        }
        let first = drain_pending(&mut conn, "silas");
        assert_eq!(first.len(), DRAIN_CAP, "first call drains at most the cap");
        assert_eq!(first[0].content, "n0", "oldest-first within the cap");
        let block = format_drain_block("silas", &first).expect("non-empty");
        assert!(block.contains("more pending"), "cap footer warns of remainder: {block}");
        // The next call drains the remaining 5 — the backlog catches up.
        let second = drain_pending(&mut conn, "silas");
        assert_eq!(second.len(), 5, "remaining backlog drains on the next call");
        assert_eq!(second[0].content, format!("n{DRAIN_CAP}"), "continues oldest-first");
    }

    #[test]
    fn headless_act_context_is_noop() {
        // #3218 (Kade, #3318 lesson): inside act/werk.yml the drain must hard
        // no-op or it stalls the pipeline. Guard fires before any DB open, so the
        // db_path is irrelevant.
        std::env::set_var("ACT", "true");
        let out = drain_block_from_db("silas", "/nonexistent/messages.db");
        std::env::remove_var("ACT");
        assert!(out.is_none(), "headless (ACT) drain is a hard no-op");
    }

    #[test]
    fn fifo_tiebreak_by_id_within_same_second() {
        // created_at is second-resolution; two nudges in the same second must
        // still drain in arrival (id) order, not arbitrary.
        let mut conn = setup_db();
        insert(&conn, "nudge", "wren", "silas", "earlier-id", "2026-06-13 10:00:01");
        insert(&conn, "nudge", "kade", "silas", "later-id", "2026-06-13 10:00:01");

        let drained = drain_pending(&mut conn, "silas");
        let order: Vec<&str> = drained.iter().map(|n| n.content.as_str()).collect();
        assert_eq!(order, vec!["earlier-id", "later-id"], "id breaks created_at ties FIFO");
    }

    #[test]
    fn format_block_none_when_empty() {
        assert!(format_drain_block("silas", &[]).is_none(), "nothing pending → no block");
    }

    #[test]
    fn format_block_preserves_fifo_and_names_fields() {
        let nudges = vec![
            PendingNudge { id: 1, from: "wren".into(), content: "review #3218".into(), trace_id: Some("ntr-1".into()) },
            PendingNudge { id: 2, from: "kade".into(), content: "gather please".into(), trace_id: None },
        ];
        let block = format_drain_block("silas", &nudges).expect("non-empty");
        assert!(block.contains("2 for silas"), "header counts: {block}");
        assert!(block.contains("FIFO"), "header states FIFO order: {block}");
        // oldest (id 1) must appear before newer (id 2)
        let wren_at = block.find("from wren").expect("wren line");
        let kade_at = block.find("from kade").expect("kade line");
        assert!(wren_at < kade_at, "FIFO order preserved in the block");
        assert!(block.contains("review #3218"), "content shown");
        assert!(block.contains("ntr-1"), "trace shown");
        assert!(block.contains("(-)"), "missing trace renders as '-'");
    }
}
