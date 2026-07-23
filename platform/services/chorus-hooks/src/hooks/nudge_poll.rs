//! #2435 — nudge poll primitive.
//!
//! Receiver-side of the one-canonical-path design. Given chorus.log and a target
//! role, return the unread set: `nudge.emitted` events addressed to this role
//! without a matching `nudge.surfaced` for the same role. Windowed fold (bounded
//! by event count) to stay under the 100ms p95 budget established by the 0.1
//! latency spike — full-log fold is 620–860ms at current spine scale.
//!
//! Event name rationale (Kade's 0.3 audit): the surface-ack event is named
//! `nudge.surfaced` to avoid collision with `role_state`'s drain event
//! `nudge.acknowledged`, which carries a count payload and different semantics.
//!
//! Consumers (planned): context_inject.rs during UserPromptSubmit, a PostToolUse
//! hook, and role_state.rs on transitions to idle/waiting/building. All three
//! surface unread via the same primitive — a single read path for the single
//! write path established by `nudge.rs` wedge 1.

#[cfg(test)]
use std::fs;
use std::path::Path;

/// One pending nudge that has been emitted but not yet surfaced to its target.
#[derive(Debug, Clone)]
pub struct UnreadNudge {
    pub trace_id: String,
    pub from: String,
    pub content: String,
}

/// #3607 built the bounded tail read here after the 117MB-unrotated-log latency
/// canary fired; #3670 promoted it to `shared::log_tail` after the same
/// whole-file pattern at four OTHER call sites OOM'd the Library mac (28.5GB).
/// This module now just uses the shared helper.
use crate::shared::log_tail::read_log_tail;

/// Return the unread set for `role`: nudge.emitted events addressed to this
/// role with no matching nudge.surfaced. Scans at most the last `window_events`
/// lines of the log (tail-read — never the whole file). Returns newest first.
/// Empty on missing log.
pub fn fetch_unread(role: &str, log_path: &Path, window_events: usize) -> Vec<UnreadNudge> {
    let Some(content) = read_log_tail(log_path) else {
        return Vec::new();
    };

    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(window_events);
    let window = &all_lines[start..];

    let mut emitted: Vec<UnreadNudge> = Vec::new();
    let mut surfaced: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in window {
        // Cheap filter before JSON parse — most lines are irrelevant.
        let is_emitted = line.contains(r#""event":"nudge.emitted""#);
        // #3235: a surfaced event clears THIS role's unread when it was surfaced
        // FOR this role. Two emitters, two shapes: the augment path (and the test
        // fixtures) carry the recipient as the top-level "role"; the pulse
        // delivery-worker (live osascript delivery) carries the recipient in a
        // "to" field with role="pulse" (the service). Matching only "role" missed
        // the live delivery → the nudge re-surfaced in the context block → it
        // rendered twice. Match either, so a delivered nudge folds out.
        let is_surfaced_for_role = line.contains(r#""event":"nudge.surfaced""#)
            && (line.contains(&format!(r#""role":"{}""#, role))
                || line.contains(&format!(r#""to":"{}""#, role))
                || line.contains(&format!("to={}", role)));
        if !is_emitted && !is_surfaced_for_role {
            continue;
        }

        if is_surfaced_for_role {
            if let Some(tid) = extract_trace_id(line) {
                surfaced.insert(tid);
            }
            continue;
        }

        // nudge.emitted — parse payload crammed into the first kv pair's value.
        // Shape (from nudge.rs): from=<sender>,to=<target>,chars=N,trace=ntr-...,content=<preview>
        // The sender is also available as top-level "role" field on the event.
        let Some(to) = extract_field(line, "to=") else { continue; };
        if to != role { continue; }

        let Some(trace_id) = extract_trace_id(line) else { continue; };
        // Sender: chorus_log puts it in the top-level "role" JSON field. The `from=`
        // prefix gets consumed when chorus_log packs the first kv pair, so it isn't
        // literally in the line as `from=X,` — parse the JSON role field instead.
        let from = extract_json_string_field(line, "role").unwrap_or_default();
        let content = extract_content_preview(line);

        emitted.push(UnreadNudge { trace_id, from, content });
    }

    // Unread = emitted minus surfaced. Preserve newest-first by iterating the
    // emitted vec in reverse (we pushed oldest-first during the forward scan).
    emitted
        .into_iter()
        .rev()
        .filter(|n| !surfaced.contains(&n.trace_id))
        .collect()
}

/// Extract a comma-bounded field like `to=wren` or `from=silas` from the JSON
/// value string where nudge.rs packs the payload. Returns the value portion.
fn extract_field(line: &str, prefix: &str) -> Option<String> {
    let start = line.find(prefix)? + prefix.len();
    let tail = &line[start..];
    let end = tail.find(',').unwrap_or(tail.len());
    Some(tail[..end].to_string())
}

/// Extract trace id. Handles both `trace=ntr-...,` (emitted payload) and
/// `"trace":"ntr-...,` (surfaced event top-level field).
fn extract_trace_id(line: &str) -> Option<String> {
    // `trace=` (emitted packed payload) · `"trace_id":"` (pulse delivery-worker
    // JSON surfaced, #3235) · `"trace":"` (augment surfaced). Try trace_id before
    // trace so the longer key wins on the pulse form ("trace" is not a substring
    // of "trace_id" with the `":"` suffix, but order it explicitly to be safe).
    extract_field(line, "trace=")
        .or_else(|| extract_json_string_field(line, "trace_id"))
        .or_else(|| extract_json_string_field(line, "trace"))
}

/// Extract a top-level JSON string field like `"role":"value"` — stops at
/// either a quote or a comma inside the value (chorus_log packs extra kv
/// pairs into the value of the first field, separated by commas).
fn extract_json_string_field(line: &str, key: &str) -> Option<String> {
    let needle = format!(r#""{}":""#, key);
    let start = line.find(&needle)? + needle.len();
    let tail = &line[start..];
    let end = tail.find([',', '"']).unwrap_or(tail.len());
    Some(tail[..end].to_string())
}

/// Extract the content preview that nudge.rs packs as the last kv pair.
/// Shape: `...content=<preview>"}` — runs to closing quote.
fn extract_content_preview(line: &str) -> String {
    let Some(start) = line.find("content=") else { return String::new() };
    let start = start + "content=".len();
    let tail = &line[start..];
    let end = tail.find('"').unwrap_or(tail.len());
    tail[..end].to_string()
}

/// Augment an envelope string with a pending-nudge block when there are unread
/// nudges for `role`. Reads chorus.log from `log_path`, formats the top `limit`
/// pending nudges, emits nudge.surfaced for each, and returns the augmented
/// envelope. Returns unchanged when nothing is pending.
///
/// During #2435 floor: envelope runs unconditionally as belt-and-suspenders
/// alongside inject. Successful inject emits nudge.surfaced on the receiver
/// side so the fold (emitted − surfaced) naturally skips anything inject
/// already delivered — no duplicate surfacing in practice. When the canonical
/// spine-tick-poller ships, envelope becomes backup behind tick-poller; inject
/// retires then.
pub fn augment_envelope_with_nudges(
    role: &str,
    envelope: &str,
    log_path: &Path,
    window_events: usize,
    limit: usize,
) -> String {
    let unread = fetch_unread(role, log_path, window_events);
    let Some(block) = format_unread_block(role, &unread, limit) else {
        return envelope.to_string();
    };
    mark_surfaced(role, &unread[..unread.len().min(limit)]);
    format!("{}\n{}", envelope, block)
}

/// Emit a nudge.surfaced spine event for each nudge — the canonical receipt
/// that closes the fold (`unread = emitted - surfaced`). Called by the wiring
/// layer after it has placed the nudges in the envelope where the role will
/// see them. lag_ms is left out of the payload for now; when emit_ts is
/// available on the UnreadNudge (wedge 4 extension) the caller includes it.
///
/// Emission goes through the chorus-log shell script — same route used by
/// nudge.emitted and every other spine event — so both sides of the fold
/// live on one canonical write path (#2435). Goes via shared::state_paths
/// so this primitive is reachable from either chorus-hooks binary.
pub fn mark_surfaced(role: &str, nudges: &[UnreadNudge]) {
    let log_script = crate::shared::state_paths::chorus_log_script();
    if !std::path::Path::new(&log_script).exists() {
        return;
    }
    for n in nudges {
        let _ = std::process::Command::new(&log_script)
            .args([
                "nudge.surfaced",
                role,
                &format!("trace={},from={},to={}", n.trace_id, n.from, role),
            ])
            .output();
    }
}

/// Format a set of unread nudges into a markdown block for envelope injection.
/// Returns None when there's nothing to say — caller skips the section entirely.
/// Caps at `limit` lines (oldest visible) with a remaining-count footer when
/// truncated; agents shouldn't scroll through dozens of nudges inline.
pub fn format_unread_block(role: &str, nudges: &[UnreadNudge], limit: usize) -> Option<String> {
    if nudges.is_empty() {
        return None;
    }
    let shown = nudges.len().min(limit);
    let mut out = String::new();
    out.push_str(&format!(
        "\n## Pending nudges ({} unread for {})\n",
        nudges.len(),
        role
    ));
    for n in &nudges[..shown] {
        out.push_str(&format!(
            "- from {} ({}): {}\n",
            n.from, n.trace_id, n.content
        ));
    }
    if nudges.len() > shown {
        // #2664: /api/nudge/:role/pending retired. Full list is in the spine
        // log (chorus.log) — fold nudge.emitted minus nudge.surfaced for `role`.
        out.push_str(&format!(
            "- ...and {} more — see chorus.log spine fold for {}\n",
            nudges.len() - shown,
            role
        ));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn fixture_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nudge-poll-fixture-{}-{}",
            std::process::id(),
            name,
        ));
        fs::create_dir_all(&dir).expect("fixture dir");
        dir.join("chorus.log")
    }

    fn write_log(path: &PathBuf, lines: &[&str]) {
        fs::write(path, lines.join("\n") + "\n").expect("write fixture log");
    }

    fn emitted(from: &str, to: &str, trace: &str, content: &str) -> String {
        format!(
            r#"{{"timestamp":"2026-04-21T17:00:00.000-0400","level":"info","appName":"chorus-events","component":"lifecycle","event":"nudge.emitted","role":"{}","from":"{},to={},chars={},trace={},content={}"}}"#,
            from, from, to, content.len(), trace, content
        )
    }

    fn surfaced(role: &str, trace: &str, lag_ms: u64) -> String {
        format!(
            r#"{{"timestamp":"2026-04-21T17:00:05.000-0400","level":"info","appName":"chorus-events","component":"lifecycle","event":"nudge.surfaced","role":"{}","trace":"{},lag_ms={}"}}"#,
            role, trace, lag_ms
        )
    }

    #[test]
    fn returns_empty_when_log_missing() {
        let missing = std::env::temp_dir().join("definitely-not-a-log-file-xyz");
        let _ = fs::remove_file(&missing);
        let unread = fetch_unread("silas", &missing, 1000);
        assert!(unread.is_empty(), "missing log returns empty");
    }

    #[test]
    fn returns_empty_when_no_emitted_for_role() {
        let log = fixture_path("no-target");
        write_log(&log, &[
            &emitted("wren", "kade", "ntr-001", "hello kade"),
            &emitted("kade", "wren", "ntr-002", "hi back"),
        ]);
        let unread = fetch_unread("silas", &log, 1000);
        assert!(unread.is_empty());
    }

    #[test]
    fn finds_nudge_addressed_to_role() {
        let log = fixture_path("one-unread");
        write_log(&log, &[
            &emitted("wren", "silas", "ntr-010", "please review"),
        ]);
        let unread = fetch_unread("silas", &log, 1000);
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].from, "wren");
        assert_eq!(unread[0].trace_id, "ntr-010");
        assert!(unread[0].content.contains("please review"));
    }

    #[test]
    fn excludes_surfaced_nudges() {
        let log = fixture_path("some-surfaced");
        write_log(&log, &[
            &emitted("wren", "silas", "ntr-020", "first"),
            &emitted("kade", "silas", "ntr-021", "second"),
            &surfaced("silas", "ntr-020", 1500),
        ]);
        let unread = fetch_unread("silas", &log, 1000);
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].trace_id, "ntr-021");
    }

    #[test]
    fn ignores_other_roles_surfaced_events() {
        let log = fixture_path("cross-role-surfaced");
        write_log(&log, &[
            &emitted("wren", "silas", "ntr-030", "addressed to silas"),
            &surfaced("kade", "ntr-030", 800),
        ]);
        let unread = fetch_unread("silas", &log, 1000);
        assert_eq!(unread.len(), 1, "kade surfacing trace-030 must not clear silas's unread");
    }

    #[test]
    fn live_delivery_surfaced_via_to_field_clears_unread() {
        // #3235 double-render root cause: the pulse delivery-worker emits
        // nudge.surfaced with role="pulse" (the service identity) + the recipient
        // in a "to" JSON field + trace in "trace_id". The fold only matched
        // surfaced by role==recipient and only read trace from trace=/"trace", so
        // it was doubly blind to the live delivery → it re-surfaced the nudge in
        // the context block → the nudge rendered twice. The fold must fold out a
        // surfaced whose recipient (to) == role, regardless of the emitter role.
        let log = fixture_path("pulse-delivered");
        write_log(&log, &[
            &emitted("silas", "kade", "019e-aaa", "please review"),
            r#"{"timestamp":"2026-06-04T21:02:38.536Z","event":"nudge.surfaced","role":"pulse","trace_id":"019e-aaa","id":15760,"from":"silas","to":"kade","attempt":1}"#,
        ]);
        let unread = fetch_unread("kade", &log, 1000);
        assert!(
            unread.is_empty(),
            "a live-delivered nudge (pulse surfaced, to=kade) must fold out — no double-render"
        );
    }

    #[test]
    fn respects_window_bound() {
        let log = fixture_path("windowed");
        let mut lines: Vec<String> = Vec::new();
        lines.push(emitted("wren", "silas", "ntr-old", "too old to scan"));
        for i in 0..200 {
            lines.push(format!(
                r#"{{"timestamp":"2026-04-21T17:00:00.000-0400","event":"noise.{}","role":"system"}}"#, i
            ));
        }
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        write_log(&log, &refs);
        let unread = fetch_unread("silas", &log, 100);
        assert!(unread.is_empty(), "oldest nudge falls outside window");
    }

    /// Smoke test against the live chorus.log: parse must not panic on real
    /// event shapes emitted by nudge.rs wedge 1. #3606 — the latency assert
    /// moved to `latency_budget_on_synthetic_50k`: a wall-clock budget against
    /// the LIVE, unboundedly-growing log (122MB by 2026-07) under arbitrary
    /// machine load was red-by-construction — and under llvm-cov
    /// instrumentation (several× slower) it could never pass. That one assert
    /// was BOTH nightly reds: cargo "53 ok, 1 failed" and coverage rc=101.
    #[test]
    fn real_log_smoke() {
        let log_path = Path::new("/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log");
        if !log_path.exists() {
            // CI or clean environment — skip gracefully.
            return;
        }
        for role in &["silas", "wren", "kade"] {
            // No panic + plausible result is the whole contract; count depends
            // on live state.
            let _ = fetch_unread(role, log_path, 50_000).len();
        }
    }

    /// AC 0.1 latency budget on a CONTROLLED input: 50k synthetic lines in a
    /// tmp fixture, so the measurement is the algorithm — not whatever size
    /// the live log happens to be or what else the box is doing. Wall-clock
    /// assert skipped under coverage instrumentation (cfg(coverage), set by
    /// cargo-llvm-cov) where timing is meaningless; the parse still runs
    /// there so coverage is collected.
    #[test]
    fn latency_budget_on_synthetic_50k() {
        let log = fixture_path("latency-50k");
        let mut lines: Vec<String> = Vec::with_capacity(50_000);
        for i in 0..49_999 {
            lines.push(format!(
                r#"{{"timestamp":"2026-04-21T17:00:00.000-0400","event":"noise.{}","role":"system"}}"#,
                i
            ));
        }
        lines.push(emitted("wren", "silas", "ntr-latency", "one real nudge in the noise"));
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        write_log(&log, &refs);

        let t = std::time::Instant::now();
        let unread = fetch_unread("silas", &log, 50_000);
        let elapsed_ms = t.elapsed().as_millis();

        assert_eq!(unread.len(), 1, "the one real nudge must be found among 50k lines");
        if !cfg!(coverage) {
            assert!(
                elapsed_ms < 200,
                "fetch_unread(silas, 50k synthetic) took {}ms — AC 0.1 budget is 100ms p95 / 200ms hard ceiling",
                elapsed_ms
            );
        }
    }

    // --- format_unread_block: pure formatter tests ---

    fn nudge(from: &str, trace: &str, content: &str) -> UnreadNudge {
        UnreadNudge {
            trace_id: trace.into(),
            from: from.into(),
            content: content.into(),
        }
    }

    // --- augment_envelope_with_nudges: unconditional during floor ---

    #[test]
    fn augment_is_noop_when_nothing_pending() {
        let log = fixture_path("augment-empty-pending");
        write_log(&log, &[
            &emitted("wren", "kade", "ntr-bb", "not for silas"),
        ]);
        let envelope = "<chorus-context>stuff</chorus-context>";
        let out = augment_envelope_with_nudges("silas", envelope, &log, 1000, 5);
        assert_eq!(out, envelope, "no pending → envelope unchanged");
    }

    #[test]
    fn augment_appends_block_when_pending() {
        let log = fixture_path("augment-pending");
        write_log(&log, &[
            &emitted("wren", "silas", "ntr-cc", "please review #2435"),
        ]);
        let envelope = "<chorus-context>stuff</chorus-context>";
        let out = augment_envelope_with_nudges("silas", envelope, &log, 1000, 5);
        assert!(out.starts_with(envelope), "preserves original envelope");
        assert!(out.contains("Pending nudges"), "adds pending nudges block");
        assert!(out.contains("from wren"), "names sender");
        assert!(out.contains("ntr-cc"), "names trace");
        assert!(out.contains("please review #2435"), "includes content");
    }

    #[test]
    fn augment_skips_nudges_already_surfaced_by_inject() {
        // Floor dedup: when nudge.rs inject succeeds, it emits nudge.surfaced
        // directly (without envelope ever firing). On the receiver's next
        // UserPromptSubmit, the fold sees the surfaced event and skips.
        let log = fixture_path("augment-inject-surfaced");
        write_log(&log, &[
            &emitted("wren", "silas", "ntr-dd", "delivered via inject"),
            &surfaced("silas", "ntr-dd", 0),
        ]);
        let envelope = "<chorus-context>stuff</chorus-context>";
        let out = augment_envelope_with_nudges("silas", envelope, &log, 1000, 5);
        assert_eq!(out, envelope, "surfaced nudges must not double-appear in envelope");
    }

    #[test]
    fn format_empty_returns_none() {
        assert!(format_unread_block("silas", &[], 5).is_none());
    }

    #[test]
    fn format_one_nudge_returns_block() {
        let nudges = vec![nudge("wren", "ntr-1", "review #2435")];
        let block = format_unread_block("silas", &nudges, 5).expect("non-empty");
        assert!(block.contains("1 unread for silas"), "header: {}", block);
        assert!(block.contains("from wren"), "sender: {}", block);
        assert!(block.contains("ntr-1"), "trace: {}", block);
        assert!(block.contains("review #2435"), "content: {}", block);
        assert!(!block.contains("...and"), "no truncation footer under limit");
    }

    #[test]
    fn format_caps_at_limit_with_footer() {
        let nudges: Vec<UnreadNudge> = (0..12)
            .map(|i| nudge("wren", &format!("ntr-{}", i), "content"))
            .collect();
        let block = format_unread_block("silas", &nudges, 5).expect("non-empty");
        assert!(block.contains("12 unread"), "count reflects total, not shown: {}", block);
        assert!(block.contains("...and 7 more"), "truncation footer: {}", block);
        // Should contain the first 5 traces and NOT the rest
        for i in 0..5 {
            assert!(block.contains(&format!("ntr-{}", i)), "trace {} shown", i);
        }
        for i in 5..12 {
            assert!(!block.contains(&format!("ntr-{} ", i)),
                "trace {} should be in truncation not inline", i);
        }
    }

    #[test]
    fn format_limit_equal_to_count_no_footer() {
        let nudges: Vec<UnreadNudge> = (0..3)
            .map(|i| nudge("kade", &format!("ntr-{}", i), "msg"))
            .collect();
        let block = format_unread_block("silas", &nudges, 3).expect("non-empty");
        assert!(!block.contains("...and"), "no footer when shown == total: {}", block);
    }

    #[test]
    fn returns_newest_first_order() {
        let log = fixture_path("ordering");
        write_log(&log, &[
            &emitted("wren", "silas", "ntr-100", "first sent"),
            &emitted("kade", "silas", "ntr-101", "second sent"),
            &emitted("wren", "silas", "ntr-102", "third sent"),
        ]);
        let unread = fetch_unread("silas", &log, 1000);
        assert_eq!(unread.len(), 3);
        assert_eq!(unread[0].trace_id, "ntr-102");
        assert_eq!(unread[2].trace_id, "ntr-100");
    }
}
