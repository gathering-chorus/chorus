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

/// The inbound peer-nudges to `role` that it has NOT yet answered: nudges
/// addressed to `role` (from someone else) whose `created_at` is newer than
/// `role`'s most recent OUTBOUND nudge. Plain English: *has the agent replied to
/// anything since the last nudge it got?* If not, it owes a response.
///
/// This is the #3218 signal, and it is deliberately NOT `delivery_status`:
/// osascript flips a nudge to `delivered` within ~1s, so a pending-based gate
/// never sees a real nudge (proven the hard way on Silas — delivered_at one
/// second after created, gate never fired). `created_at` does not change on
/// delivery, so this catches the nudge regardless of osascript. It is also immune
/// to the ~7,500-row historical backlog: only the NEWEST inbound vs the newest
/// outbound matters, so nudges the agent already moved past never block.
///
/// The agent clears it simply by REPLYING — sending a nudge advances its newest
/// outbound past the inbound, so the next tool call sees nothing owed. No
/// marking, no drain, no mutation of messages.db: the reply IS the clear.
fn unanswered_inbound(conn: &Connection, role: &str) -> Vec<PendingNudge> {
    unanswered_inbound_inner(conn, role).unwrap_or_default()
}

fn unanswered_inbound_inner(conn: &Connection, role: &str) -> rusqlite::Result<Vec<PendingNudge>> {
    let mut stmt = conn.prepare(
        // PEER-ONLY (#3218 hotfix): only nudges from a real, repliable peer create
        // an owes-response debt. system / chorus-mcp / pulse / alert senders have NO
        // repliable target (the recipient enum is wren/silas/kade/jeff) — counting
        // them traps the recipient with no way to clear it (Silas's live over-trap:
        // werk-commit-fail + wedge alerts blocked every tool with no peer to answer).
        // Only peer→peer respond-first; machine notifications never trap.
        r#"SELECT id, "from", content, trace_id
           FROM messages
           WHERE "to" = ?1
             AND type = 'nudge'
             AND "from" != ?1
             AND "from" IN ('wren', 'silas', 'kade', 'jeff')
             AND nudge_class = 'r2r'
             AND nudge_expects IN ('reply', 'decision', 'action')
             AND created_at > COALESCE(
                   (SELECT MAX(created_at) FROM messages
                    WHERE "from" = ?1 AND type = 'nudge'), '')
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

/// Format drained nudges into a markdown block for additionalContext injection.
/// Returns `None` when there's nothing to inject (caller emits no block, exit 0).
/// Oldest-first order is preserved from `drain_pending` — do not re-sort.
pub fn format_drain_block(role: &str, nudges: &[PendingNudge]) -> Option<String> {
    if nudges.is_empty() {
        return None;
    }
    let mut out = format!(
        "\n## Unanswered peer nudges ({} for {} — reply to clear, oldest first)\n",
        nudges.len(),
        role
    );
    for n in nudges {
        let tid = n.trace_id.as_deref().unwrap_or("-");
        out.push_str(&format!("- from {} ({}): {}\n", n.from, tid, n.content));
    }
    if nudges.len() >= DRAIN_CAP {
        out.push_str(&format!(
            "- (oldest {DRAIN_CAP} shown — cap; the rest clear once you reply)\n"
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

/// Does `role` owe a peer a response? Returns the formatted block of unanswered
/// inbound nudges if so — the #3218 respond-first gate blocks every tool call
/// (except the reply) until this returns `None`. The agent clears it by REPLYING;
/// no marking or mutation of messages.db happens here (the reply advances the
/// agent's newest-outbound past the inbound, so the next call owes nothing).
///
/// NEVER blocks on failure: DB unreachable or any error yields `None` and the
/// tool proceeds. Headless (GITHUB_ACTIONS / ACT / CHORUS_HEADLESS) is a HARD
/// no-op — there is no terminal to reply from, so blocking would freeze the
/// act/werk.yml pipeline team-wide (the #3318 lesson; verified by test).
pub fn owes_response_block(role: &str, db_path: &str) -> Option<String> {
    owes_response(role, db_path).map(|(block, _)| block)
}

/// Structured owes-check: the formatted block PLUS a stable debt key (the oldest
/// owed nudge's row id) so the refusal-backoff (#3672) can tell "same debt, still
/// stuck" from "new debt arrived". Same never-block / headless-no-op contract as
/// `owes_response_block`.
pub fn owes_response(role: &str, db_path: &str) -> Option<(String, String)> {
    if std::env::var("GITHUB_ACTIONS").is_ok()
        || std::env::var("ACT").is_ok()
        || std::env::var("CHORUS_HEADLESS").is_ok()
    {
        return None;
    }
    let conn_lock = cached_conn(db_path)?;
    let conn = conn_lock.lock().ok()?;
    let owed = unanswered_inbound(&conn, role);
    let debt_key = owed.first().map(|n| n.id.to_string())?;
    format_drain_block(role, &owed).map(|block| (block, debt_key))
}

// --- #3672: the gate must never trap ---------------------------------------
//
// 2026-07-23, twice in one day: a role owed a reply, its session had no
// chorus_nudge_message MCP tool attached (the post-crash default), and the gate
// refused every tool INCLUDING the Bash transport for the very reply it
// demanded. Kade escaped via Bash (his variant only blocked at Stop); Silas had
// zero escape — 125 stop fires, freed only by a human. Two invariants fix the
// class:
//   1. The cure always passes — a reply attempt is never refused.
//   2. Refusal is bounded IN CODE — after the cap, the gate degrades to nagging
//      (debt stays visible via the capped Stop nag + a spine event) so the
//      session keeps its hands no matter what is broken.

/// Consecutive PreToolUse refusals allowed for one debt before degrading.
pub const PRETOOL_REFUSAL_CAP: usize = 3;
/// Consecutive Stop-hook blocks allowed for one debt before the turn may end.
pub const STOP_REFUSAL_CAP: usize = 2;

/// Is this tool call the reply the gate is demanding? True for the
/// chorus_nudge_message MCP tool under any server prefix, and for a Bash
/// invocation that carries the tool name (the chorus-mcp-call.sh transport —
/// the documented fallback for sessions without the MCP attach).
pub fn is_reply_path(tool: &str, bash_command: &str) -> bool {
    if tool.ends_with("chorus_nudge_message") {
        return true;
    }
    tool == "Bash" && bash_command.contains("chorus_nudge_message")
}

/// What the gate should do for one tool call, given the refusal count so far
/// for this (scope, role, debt). Pure — the trap regression tests drive this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// The reply path, or no debt — never refused.
    Allow,
    /// Debt owed, under the cap — refuse with the respond-first message.
    Refuse,
    /// Debt owed but the cap is spent: the reply visibly is NOT happening
    /// (broken transport, missing tool). Let the call through; keep nagging.
    Degrade,
}

pub fn gate_decision(is_reply: bool, refusals_so_far: usize, cap: usize) -> GateDecision {
    if is_reply {
        return GateDecision::Allow;
    }
    if refusals_so_far < cap {
        GateDecision::Refuse
    } else {
        GateDecision::Degrade
    }
}

/// Per-(scope, role) refusal counter, keyed to a specific debt. A new debt key
/// resets the count (fresh nudge → fresh chances to refuse); clearing the debt
/// resets everything for the role via `reset_refusals`.
static REFUSALS: OnceLock<Mutex<std::collections::HashMap<String, (String, usize)>>> =
    OnceLock::new();

fn refusal_map() -> &'static Mutex<std::collections::HashMap<String, (String, usize)>> {
    REFUSALS.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// Count of refusals BEFORE this one for (scope, role, debt_key), then
/// increment. Returns the pre-increment count (what gate_decision consumes).
pub fn note_refusal(scope: &str, role: &str, debt_key: &str) -> usize {
    let key = format!("{scope}:{role}");
    let mut map = match refusal_map().lock() {
        Ok(m) => m,
        Err(_) => return 0, // poisoned lock: fail-open (never trap on our own state)
    };
    let entry = map.entry(key).or_insert_with(|| (debt_key.to_string(), 0));
    if entry.0 != debt_key {
        *entry = (debt_key.to_string(), 0);
    }
    let before = entry.1;
    entry.1 += 1;
    before
}

/// Debt cleared for `role` — drop its counters in every scope.
pub fn reset_refusals(role: &str) {
    if let Ok(mut map) = refusal_map().lock() {
        map.retain(|k, _| !k.ends_with(&format!(":{role}")));
    }
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
                 trace_id TEXT,
                 nudge_class TEXT DEFAULT 'r2r',
                 nudge_expects TEXT DEFAULT 'none'
               );"#,
        )
        .expect("create messages table");
        conn
    }

    /// Inserts a real r2r ASK (class r2r, expects reply) — the shape that owes a
    /// response. Existing owes-tests use this so they keep trapping under the
    /// envelope filter; the a2r / expects=none cases insert their envelope raw.
    fn insert(conn: &Connection, typ: &str, from: &str, to: &str, content: &str, created_at: &str) {
        conn.execute(
            r#"INSERT INTO messages (type, "from", "to", content, created_at, delivery_status, trace_id, nudge_class, nudge_expects)
               VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, 'r2r', 'reply')"#,
            rusqlite::params![typ, from, to, content, created_at, format!("ntr-{content}")],
        )
        .expect("insert message");
    }

    /// Insert an OUTBOUND nudge from `role` (a reply) at `created_at`.
    fn reply(conn: &Connection, role: &str, to: &str, created_at: &str) {
        insert(conn, "nudge", role, to, &format!("reply-{created_at}"), created_at);
    }

    #[test]
    fn owes_when_nudged_after_last_reply() {
        let conn = setup_db();
        reply(&conn, "silas", "kade", "2026-06-13 10:00:01"); // silas's last reply
        insert(&conn, "nudge", "wren", "silas", "answer me", "2026-06-13 10:00:05"); // inbound, newer
        let owed = unanswered_inbound(&conn, "silas");
        assert_eq!(owed.len(), 1, "a nudge newer than the last reply is owed");
        assert_eq!(owed[0].content, "answer me");
    }

    #[test]
    fn delivered_nudge_still_owes() {
        // THE bug the old code had: osascript flips a nudge to 'delivered' in ~1s,
        // and the old gate keyed on delivery_status='pending', so it never fired on
        // a real peer (proven on Silas). The owes-response signal is created_at, so
        // a DELIVERED nudge newer than the last reply STILL owes — gate still fires.
        let conn = setup_db();
        conn.execute(
            r#"INSERT INTO messages (type,"from","to",content,created_at,delivery_status,delivered_at,nudge_class,nudge_expects)
               VALUES ('nudge','wren','silas','delivered but unanswered','2026-06-13 10:00:05','delivered','2026-06-13 10:00:06','r2r','reply')"#,
            [],
        ).unwrap();
        let owed = unanswered_inbound(&conn, "silas");
        assert_eq!(owed.len(), 1, "delivered != answered — osascript delivery does NOT clear the debt");
        assert_eq!(owed[0].content, "delivered but unanswered");
    }

    #[test]
    fn cleared_when_reply_is_newer() {
        let conn = setup_db();
        insert(&conn, "nudge", "wren", "silas", "answer me", "2026-06-13 10:00:05"); // inbound
        reply(&conn, "silas", "wren", "2026-06-13 10:00:06");                        // silas replies, newer
        assert!(
            unanswered_inbound(&conn, "silas").is_empty(),
            "replying clears the debt — newest outbound > inbound"
        );
    }

    #[test]
    fn backlog_excluded_only_newest_matters() {
        // The ~7,500-row history must NOT block: an old inbound the agent already
        // replied past is not owed. Only inbound newer than the last reply counts.
        let conn = setup_db();
        insert(&conn, "nudge", "wren", "silas", "old nudge", "2026-06-13 09:00:00");
        reply(&conn, "silas", "wren", "2026-06-13 09:00:01"); // silas already replied past it
        assert!(
            unanswered_inbound(&conn, "silas").is_empty(),
            "an answered-long-ago nudge does not block"
        );
    }

    #[test]
    fn no_owe_when_no_inbound() {
        let conn = setup_db();
        reply(&conn, "silas", "wren", "2026-06-13 10:00:01");
        assert!(unanswered_inbound(&conn, "silas").is_empty(), "no inbound peer nudge → owes nothing");
    }

    #[test]
    fn self_sent_not_owed() {
        let conn = setup_db();
        insert(&conn, "nudge", "silas", "silas", "talking-to-myself", "2026-06-13 10:00:05");
        assert!(unanswered_inbound(&conn, "silas").is_empty(), "a self-sent nudge is not a peer debt");
    }

    #[test]
    fn system_sender_does_not_owe() {
        // The hotfix (Silas's live over-trap): a 'nudge' from system / chorus-mcp /
        // pulse / alert has no repliable peer (recipient enum is wren/silas/kade/jeff),
        // so it must NOT trap the recipient — there is no way to clear it.
        let conn = setup_db();
        insert(&conn, "nudge", "system", "silas", "werk-commit-fail alert", "2026-06-13 10:00:05");
        insert(&conn, "nudge", "chorus-mcp", "silas", "wedge alert", "2026-06-13 10:00:06");
        assert!(
            unanswered_inbound(&conn, "silas").is_empty(),
            "system/mcp nudges create no debt — no repliable peer to answer"
        );
        // a real peer nudge still owes
        insert(&conn, "nudge", "wren", "silas", "real peer nudge", "2026-06-13 10:00:07");
        assert_eq!(unanswered_inbound(&conn, "silas").len(), 1, "a real peer nudge still traps");
    }

    /// Insert an inbound nudge with an explicit envelope (class + expects).
    fn insert_env(conn: &Connection, from: &str, to: &str, content: &str, created_at: &str, class: &str, expects: &str) {
        conn.execute(
            r#"INSERT INTO messages (type, "from", "to", content, created_at, delivery_status, nudge_class, nudge_expects)
               VALUES ('nudge', ?1, ?2, ?3, ?4, 'pending', ?5, ?6)"#,
            rusqlite::params![from, to, content, created_at, class, expects],
        )
        .expect("insert envelope message");
    }

    #[test]
    fn r2r_ask_expects_reply_traps() {
        // The shape that owes a response: a peer asking for something back.
        let conn = setup_db();
        insert_env(&conn, "kade", "silas", "review my card?", "2026-06-13 10:00:05", "r2r", "reply");
        assert_eq!(unanswered_inbound(&conn, "silas").len(), 1, "r2r + expects=reply traps");
    }

    #[test]
    fn r2r_ack_expects_none_does_not_trap() {
        // THE chip-and-dale fix: a peer ack/fyi (expects=none) must NOT trap, or
        // every "thanks, got it" re-opens the loop. This is the red→green case —
        // the peer-only gate trapped this kade ack with no expects filter.
        let conn = setup_db();
        insert_env(&conn, "kade", "silas", "thanks, got it", "2026-06-13 10:00:05", "r2r", "none");
        assert!(
            unanswered_inbound(&conn, "silas").is_empty(),
            "r2r + expects=none is a terminal ack — never traps"
        );
    }

    #[test]
    fn a2r_alert_does_not_trap_regardless_of_expects() {
        // An alert into a terminal can't be answered. class=a2r never traps, even
        // if some emitter mislabels expects. Belt to the peer-only suspenders.
        let conn = setup_db();
        insert_env(&conn, "system", "silas", "eventloop blocked", "2026-06-13 10:00:05", "a2r", "reply");
        assert!(
            unanswered_inbound(&conn, "silas").is_empty(),
            "a2r alerts never trap — no repliable peer"
        );
    }

    #[test]
    fn nudge_type_only() {
        let conn = setup_db();
        insert(&conn, "chat", "wren", "silas", "chatter", "2026-06-13 10:00:05");
        insert(&conn, "board-event", "system", "silas", "an-event", "2026-06-13 10:00:06");
        assert!(unanswered_inbound(&conn, "silas").is_empty(), "only type='nudge' creates a debt");
        insert(&conn, "nudge", "wren", "silas", "real-nudge", "2026-06-13 10:00:07");
        assert_eq!(unanswered_inbound(&conn, "silas").len(), 1, "a real nudge does");
    }

    #[test]
    fn recipient_scoped() {
        let conn = setup_db();
        insert(&conn, "nudge", "wren", "kade", "for-kade", "2026-06-13 10:00:05");
        assert!(unanswered_inbound(&conn, "silas").is_empty(), "kade's nudge is not silas's debt");
        assert_eq!(unanswered_inbound(&conn, "kade").len(), 1, "it is kade's");
    }

    #[test]
    fn owed_set_is_oldest_first_and_capped() {
        let conn = setup_db();
        reply(&conn, "silas", "wren", "2026-06-13 09:59:00"); // old last reply
        for i in 0..(DRAIN_CAP + 5) {
            insert(&conn, "nudge", "wren", "silas", &format!("n{i:02}"), &format!("2026-06-13 10:{i:02}:00"));
        }
        let owed = unanswered_inbound(&conn, "silas");
        assert_eq!(owed.len(), DRAIN_CAP, "owed set is capped at DRAIN_CAP");
        assert_eq!(owed[0].content, "n00", "oldest first");
        assert!(
            format_drain_block("silas", &owed).unwrap().contains("clear once you reply"),
            "cap footer present"
        );
    }

    #[test]
    fn respond_first_end_to_end() {
        // The full gate behavior against a real temp DB, no deploy: a peer nudges
        // silas → silas owes (block) — and crucially the inbound is DELIVERED
        // (osascript), still blocks. silas replies → owes nothing (resume).
        let conn = setup_db();
        conn.execute(
            r#"INSERT INTO messages (type,"from","to",content,created_at,delivery_status,nudge_class,nudge_expects)
               VALUES ('nudge','wren','silas','answer me first','2026-06-13 10:00:05','delivered','r2r','reply')"#,
            [],
        ).unwrap();
        // 1. owes → the gate blocks every non-reply tool (the case the OLD gate missed)
        assert!(
            format_drain_block("silas", &unanswered_inbound(&conn, "silas")).is_some(),
            "delivered-but-unanswered nudge BLOCKS"
        );
        // 2. silas replies (any outbound nudge, newer)
        reply(&conn, "silas", "wren", "2026-06-13 10:00:06");
        // 3. owes nothing → work resumes
        assert!(
            format_drain_block("silas", &unanswered_inbound(&conn, "silas")).is_none(),
            "after the reply, no debt — work resumes"
        );
    }

    #[test]
    fn headless_act_is_noop() {
        // owes_response_block must hard no-op headless or it freezes the pipeline
        // (block every tool in a context that can never reply). Guard precedes any
        // DB open, so the path is irrelevant.
        std::env::set_var("ACT", "true");
        let out = owes_response_block("silas", "/nonexistent/messages.db");
        std::env::remove_var("ACT");
        assert!(out.is_none(), "headless (ACT): owes_response_block must no-op");
    }

    #[test]
    fn format_block_none_when_empty() {
        assert!(format_drain_block("silas", &[]).is_none(), "nothing owed → no block");
    }

    #[test]
    fn format_block_names_fields_oldest_first() {
        let nudges = vec![
            PendingNudge { id: 1, from: "wren".into(), content: "review #3218".into(), trace_id: Some("ntr-1".into()) },
            PendingNudge { id: 2, from: "kade".into(), content: "gather please".into(), trace_id: None },
        ];
        let block = format_drain_block("silas", &nudges).expect("non-empty");
        assert!(block.contains("2 for silas"), "header counts: {block}");
        assert!(block.contains("reply to clear"), "header says reply clears it: {block}");
        let wren_at = block.find("from wren").expect("wren line");
        let kade_at = block.find("from kade").expect("kade line");
        assert!(wren_at < kade_at, "oldest first preserved");
        assert!(block.contains("review #3218"), "content shown");
        assert!(block.contains("ntr-1"), "trace shown");
        assert!(block.contains("(-)"), "missing trace renders as '-'");
    }
}
