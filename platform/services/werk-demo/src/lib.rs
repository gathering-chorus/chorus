// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! werk-demo — `/demo` proving gate v2 (card #3046).
//!
//! Folds the /demo skill into the werk binary. The ACT is build → deploy → verify,
//! which invokes the shipped verbs werk-build (#3061) + werk-deploy (#3062) rather
//! than re-implementing them; demo gates that act with validate → AC-preflight →
//! gate-chain → DEC-048 non-builder-confirm. Record = logs (card+trace jsonl, Loki
//! ingests) + gh per-card status `chorus/demo/<card>`; NO evidence token.
//!
//! Self-contained: std only + a direct libc `flock` extern; git / cards / werk-build
//! / werk-deploy as subprocesses. Zero dependency on any other chorus code (ADR-032).
//!
//! INCREMENT 1 (#3046): primitives + the act-spine + the demo-skill gates that are
//! pure-checkable (validate, AC-preflight, gate-chain presence, DEC-048). The deeper
//! fold — the bash smoke gates' internals + the 4 demo_* hooks (preflight/provenance/
//! show/trace) — is the NAMED next increment, ported against an explicit check-map so
//! nothing drops silently (the card's central caution).

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// #3513 — the ONE shared failure classifier (failure_class / fail_extra).
include!("../../shared/failure_class.rs");

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
}
const LOCK_EX_NB: i32 = 0x02 | 0x04; // LOCK_EX | LOCK_NB
const LOCK_UN: i32 = 0x08;

pub type R<T> = Result<T, String>;

// --- pure helpers (unit-tested) ---


/// #4502 — the run's trace: the pipeline's CHORUS_TRACE_ID, or a fresh one. A
/// demo enqueued alone ("more", `dagu enqueue demo -- CARD= ROLE=`) passes no
/// TRACE, and dagu sets the env to "" — empty is no trace, never the trace.
fn run_trace() -> String {
    pick_trace(env::var("CHORUS_TRACE_ID").ok())
}

/// Pure half of [`run_trace`]: a set, non-blank trace wins; else a fresh one.
pub fn pick_trace(given: Option<String>) -> String {
    given.filter(|t| !t.trim().is_empty()).unwrap_or_else(trace_id)
}

pub fn trace_id() -> String {
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}", ns, std::process::id())
}

pub fn jsonl_line(ts: u128, event: &str, role: &str, card: u64, trace: &str, extra: &str) -> String {
    format!(
        "{{\"ts\":{},\"event\":\"{}\",\"role\":\"{}\",\"card_id\":{},\"trace_id\":\"{}\"{}}}\n",
        ts, event, role, card, trace, extra
    )
}

/// AC completeness from a `cards view` body — (checked, total). Ported from the
/// /demo Step 1.5 AC pre-flight (count `- [x]` vs `- [ ]`).
pub fn ac_counts(card_view: &str) -> (usize, usize) {
    let mut checked = 0usize;
    let mut total = 0usize;
    for line in card_view.lines() {
        let t = line.trim_start();
        if t.starts_with("- [x]") {
            checked += 1;
            total += 1;
        } else if t.starts_with("- [ ]") {
            total += 1;
        }
    }
    (checked, total)
}

/// #3281 — build the chorus-log argv for a REFUSAL spine emit (pure, testable).
/// Agent-side blocks (a demo refusing on wrong-status / no-ac) must become
/// countable: the event is named with the `.refused` suffix the pain board
/// counts (logs-query PAIN_EVENT_SUFFIXES), and carries role + a `reason=` field
/// so the rollup groups by role·event·reason. Mirrors the jsonl witness reason
/// already written at the refusal points, but onto the SPINE so the pain board —
/// which reads chorus.log, not werk-demo.jsonl — can see it.
pub fn refuse_spine_args(event: &str, role: &str, card: u64, trace: &str, reason: &str) -> Vec<String> {
    vec![
        event.to_string(),
        role.to_string(),
        format!("card={}", card),
        format!("trace={}", trace),
        format!("reason={}", reason),
    ]
}

/// #3237 — the gates that must have run before a demo verdict can be recorded.
/// Gates moved to the /demo SKILL layer (#3116) as LLM subagents; this is the
/// enforcement contract that makes them non-optional — the binary refuses a
/// verdict until each has left a demo.gate.result in the witness.
pub const REQUIRED_GATES: [&str; 5] = ["product", "code", "quality", "arch", "ops"];


/// #3352 — the demo ceremony as an INVARIANT ordering, verb-enforced:
///   gates recorded → gathers sent → gathers REPLIED → announce → go.
/// Jeff's spec verbatim (2026-06-11): "slow / often skipped gates; role feedback
/// skipped or failed; announce happens b4 role feedback so go precedes that."
/// Behavioral discipline failed 4× across 2 roles (3282, 3334, 3269, 3343); the
/// only durable fix is structural. announce_ready_full is the single ready
/// predicate: every gate recorded AND every peer's 4-question gather REPLIED.
/// skip (act/headless/test-suite) skips BOTH identically — scoped at birth,
/// the #3318 lesson (never land-breaking-then-scope).
/// #3365 — ROUND identity: Jeff's five steps run per ROUND, not per card.
/// A round is the commit sha being demoed (the werk's HEAD). Every gate,
/// gather, and announce record carries it; every check matches on it — so a
/// card with last round's records cannot land this round's commits unreviewed
/// (the hole that let #3352's own final round close without feedback).
/// CHORUS_DEMO_ROUND overrides for tests; git failure yields "round-unknown",
/// which never matches a recorded round by construction.
pub fn current_round(role: &str, card: u64) -> String {
    if let Ok(r) = env::var("CHORUS_DEMO_ROUND") {
        if !r.trim().is_empty() { return r.trim().to_string(); }
    }
    let werk_base = env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
        format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
    });
    // #3305 — try the invoker's own werk first (the demoer fast path), then fall
    // back to the card's werk under ANY role. A card has exactly one werk; a PEER
    // recording a gather on the demoer's card previously probed <invoker>-<card>
    // (nonexistent) and minted "round-unknown" — a record structurally invisible
    // to the announce gate. That was face two of the #3305 family (live specimen:
    // silas's 13:11 reply on kade's #3375, round-unknown on the witness).
    let own = format!("{}/{}-{}", werk_base, role, card);
    if let Some(r) = git_short12(&own) { return r; }
    round_for_card(&werk_base, card)
}

/// #4774 — canonical agent slot order for fallback and port-slot stability.
pub const FALLBACK_AGENT_ROLES: [&str; 5] = [
    "silas",
    "kade",
    "wren",
    "abby-normal",
    "borg-queen",
];

/// #4774 — pure parser for `/v1/roles/roles` JSON. Extracts `"roleKind": "agent"`
/// role names in slot order and appends any newly added agent roles.
pub fn roles_from_door_json(body: &str) -> Option<Vec<String>> {
    let data_idx = body.find("\"data\"")?;
    let mut rest = &body[data_idx..];
    let mut found = Vec::new();
    while let Some(open) = rest.find('{') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find('}') else {
            break;
        };
        let obj = &after_open[..close];
        if json_str_field(obj, "roleKind").as_deref() == Some("agent") {
            if let Some(name) = json_str_field(obj, "name") {
                if !name.is_empty() && !found.contains(&name) {
                    found.push(name);
                }
            }
        }
        rest = &after_open[close + 1..];
    }
    if found.is_empty() {
        return None;
    }
    let mut ordered = Vec::with_capacity(found.len());
    for slot_role in FALLBACK_AGENT_ROLES {
        if found.iter().any(|r| r == slot_role) {
            ordered.push(slot_role.to_string());
        }
    }
    let mut extras: Vec<String> = found
        .into_iter()
        .filter(|r| !FALLBACK_AGENT_ROLES.contains(&r.as_str()))
        .collect();
    extras.sort();
    ordered.extend(extras);
    Some(ordered)
}

/// #4774 — agent roles resolved dynamically from `/v1/roles/roles` with fallback
/// to `FALLBACK_AGENT_ROLES`.
pub fn known_agent_roles() -> &'static [&'static str] {
    static ROLES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    ROLES.get_or_init(|| {
        let door = env::var("ATHENA_MAKE_URL")
            .or_else(|_| env::var("CHORUS_DOOR"))
            .unwrap_or_else(|_| "http://127.0.0.1:3360".to_string());
        let url = format!("{}/v1/roles/roles?limit=500", door.trim_end_matches('/'));
        let resolved = run("curl", &["-s", "-f", "--max-time", "2", &url])
            .ok()
            .and_then(|body| roles_from_door_json(&body))
            .unwrap_or_else(|| FALLBACK_AGENT_ROLES.iter().map(|s| s.to_string()).collect());
        resolved
            .into_iter()
            .map(|s| -> &'static str { Box::leak(s.into_boxed_str()) })
            .collect()
    })
}

/// Resolve the round (HEAD short-12) from the CARD's werk, whichever role owns
/// it. "round-unknown" only when no role has a werk for the card.
pub fn round_for_card(werk_base: &str, card: u64) -> String {
    for r in known_agent_roles() {
        if let Some(sha) = git_short12(&format!("{}/{}-{}", werk_base, r, card)) {
            return sha;
        }
    }
    "round-unknown".to_string()
}

fn git_short12(werk: &str) -> Option<String> {
    if !Path::new(werk).is_dir() { return None; }
    std::process::Command::new("git")
        .args(["-C", werk, "rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// #3459 — patch-id of the card's werk (whichever role owns it), mirroring
/// `round_for_card`. Recorded on `demo.presented` so werk-merge can accept a
/// content-preserving rebase: the sha churns when a peer lands, the patch-id does
/// not. Empty string when uncomputable — the merge side then refuses (never
/// silently allows).
/// #4453 — the presenting role's werk first. A fixed wren, silas, kade order
/// made a peer's leftover werk for the same card (silas-4446) the presented
/// patch-id for three rounds, so every go on #4446 refused (2026-10-07 12:42,
/// 13:21). Another role's werk is only a fallback, as in current_round.
pub fn patch_id_for_card(werk_base: &str, role: &str, card: u64) -> String {
    let own = format!("{}/{}-{}", werk_base, role, card);
    if Path::new(&own).is_dir() {
        return git_patch_id(&own);
    }
    for r in known_agent_roles() {
        let werk = format!("{}/{}-{}", werk_base, r, card);
        if Path::new(&werk).is_dir() {
            return git_patch_id(&werk);
        }
    }
    String::new()
}

/// `git patch-id --stable` of `merge-base(origin/main,HEAD)..HEAD`. The `diff |
/// patch-id` pipe runs via `bash -c` (subprocess only). Empty on any failure.
fn git_patch_id(werk: &str) -> String {
    if !Path::new(werk).is_dir() { return String::new(); }
    let base = std::process::Command::new("git")
        .args(["-C", werk, "merge-base", "origin/main", "HEAD"])
        .output().ok().filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if base.is_empty() { return String::new(); }
    // `git diff | git patch-id` wired NATIVELY through std Stdio — no `bash -c`.
    // An atomic verb must not depend on a shell on the runtime PATH (#3459: the
    // shell-wrap broke lands in the daemon env; ADR-032 §1 — git subprocess, no shell).
    let mut diff = match std::process::Command::new("git")
        .args(["-C", werk, "diff", &format!("{}..HEAD", base)])
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let diff_stdout = match diff.stdout.take() {
        Some(s) => s,
        None => return String::new(),
    };
    let pid = std::process::Command::new("git")
        .args(["patch-id", "--stable"])
        .stdin(diff_stdout)
        .output().ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().next().map(|t| t.to_string()))
        .unwrap_or_default();
    let _ = diff.wait();
    pid
}

/// A witness line matches the current round iff it carries this round's key.
/// Pre-#3365 records have NO round field and therefore match nothing — old
/// evidence ages out structurally the moment this lands.
fn line_in_round(line: &str, round: &str) -> bool {
    line.contains(&format!("\"round\":\"{}\"", round))
}

/// #3461 — a witness line is VALID for this proving round iff its round sha
/// matches OR its patch-id matches. The sha churns on a content-preserving rebase
/// (a peer landing → the werk re-bases → new HEAD sha → new round), but the
/// patch-id (merge-base..HEAD diff) does NOT. So a gate result / gather reply
/// recorded before the churn still counts after it — the demo stops re-eliciting
/// the SAME approvals every rebase (the #3454 re-nudge loop). Empty patch_id =
/// round-only (no false match); mirrors #3459's land-gate fallback for the
/// gather/gate gates. Pure + unit-tested.
fn line_keyed(line: &str, round: &str, patch_id: &str) -> bool {
    // #3944 — CONTENT WINS. When both sides carry a patch id, the patch id decides:
    // it identifies the tree that was actually demoed. The old rule was
    // `round OR patch`, which passed whenever the ROUND matched — so a commit added
    // after the demo (same round, new patch) still read as proven, and a land could
    // merge content Jeff never saw. The patch id was in the signature and ignored.
    //
    // Rebase tolerance (#3461) is preserved and is the reason patch is checked FIRST:
    // a content-preserving rebase churns the round but keeps the patch, so Jeff's go
    // survives. What changes is only the inverse case — same round, different content
    // — which must refuse.
    let line_patch = extract_field(line, "patch_id");
    if !patch_id.is_empty() {
        if let Some(lp) = line_patch {
            return lp == patch_id;
        }
    }
    line_in_round(line, round)
}

/// Pull a string field's value out of a witness line without a JSON parser (the verb
/// is std-only, ADR-032 §6). Returns None when the field is absent.
fn extract_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    let key = format!("\"{}\":\"", field);
    let start = line.find(&key)? + key.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// #3461 — the current werk's patch-id (mirrors current_round). I/O (git), so
/// callers of the pure matchers compute it and pass it in; the matchers stay pure.
pub fn current_patch_id(role: &str, card: u64) -> String {
    let werk_base = env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
        format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
    });
    patch_id_for_card(&werk_base, role, card)
}


pub fn announce_ready_full(witness: &str, card: u64, role: &str, round: &str, patch_id: &str, skip: bool) -> bool {
    skip || (gates_missing(witness, card, round, patch_id).is_empty() && gathers_missing(witness, card, role, round, patch_id).is_empty())
}

/// #3519 — THE WITNESS-GATED LAND predicate. demo() present-and-exits (ALWAYS exit 2,
/// #3279/#3511) and so can never signal "proven" through its exit code; but werk.yml gated
/// the merge on demo exit-0 (#3499). The two contradict → the merge step never fired →
/// nothing landed. This is the predicate the merge keys on INSTEAD: the land is proven iff,
/// for THIS patch, the witness shows every required gate + both peer gathers replied
/// (announce_ready_full) AND Jeff's recorded go (jeff_go_recorded). All matching is patch-
/// keyed (line_keyed, #3461) so it survives the concurrent rebases that churn the round-sha.
/// Pure + unit-tested (tests/3519-verdict.rs). Reconciles #3499 (gate the land) with #3511
/// (present-and-exit): demo presents, the go records, verdict reads the witness, merge lands.
pub fn verdict_proven(witness: &str, card: u64, role: &str, round: &str, patch_id: &str) -> bool {
    announce_ready_full(witness, card, role, round, patch_id, false)
        && jeff_go_recorded(witness, card, round, patch_id)
}

/// #4502 — the agent roles that review each other's demos: the roles door's
/// teammates minus jeff (the prover, not a peer), measured 2026-10-18 11:50:
/// abby-normal, kade, silas, wren — the roles with a CLAUDE.md source under
/// designing/claudemd/roles/. The persona read at review time is the generated
/// roles/<name>/CLAUDE.md, or AGENTS.md for abby-normal (she runs on agy). The
/// drift test `peer_roles_match_the_role_dirs_with_a_persona` fails when a role
/// is added or removed without this list.
pub const PEER_ROLES: [&str; 4] = ["wren", "silas", "kade", "abby-normal"];

/// #4691 — the agent roles that prove the land instead of reviewing demos:
/// borg-queen signs the verdict after deploy (cicd-land skill_borg_queen_proves),
/// so she is not owed a review step and does not give one. Together with
/// PEER_ROLES this is every role under designing/claudemd/roles/; the same
/// drift test fails when a role dir is in neither list.
pub const PROVER_ROLES: [&str; 1] = ["borg-queen"];

/// The peer roles owed a review for this demo: every agent role but the
/// demoer. Every builder gets the same count (3), so the pipeline's review
/// steps are fixed: run-review 1..3. (jeff is the prover, not a peer.)
pub fn gather_peers(role: &str) -> Vec<&'static str> {
    PEER_ROLES.into_iter().filter(|r| *r != role).take(3).collect()
}

/// #4502 — a peer's persona: roles/<peer>/CLAUDE.md, else AGENTS.md (abby-normal).
pub fn peer_persona_path(home: &Path, peer: &str) -> std::path::PathBuf {
    let claude = home.join(format!("roles/{}/CLAUDE.md", peer));
    if claude.is_file() { claude } else { home.join(format!("roles/{}/AGENTS.md", peer)) }
}

/// Peers with NO demo.gather.replied recorded for the card. Empty = both
/// peers' feedback is IN — the announce may fire. Mirrors gates_missing
/// (comma-terminated card match, same anti-collision rule). A gather reply is
/// recorded via `werk-demo gather <card> <peer> replied` when the peer's ACK
/// arrives — evidence on the witness, never a model claim.
pub fn gathers_missing(witness: &str, card: u64, role: &str, round: &str, patch_id: &str) -> Vec<&'static str> {
    let card_key = format!("\"card_id\":{},", card);
    gather_peers(role)
        .into_iter()
        .filter(|peer| {
            let peer_key = format!("\"peer\":\"{}\"", peer);
            !witness.lines().any(|l| {
                l.contains("\"event\":\"demo.gather.replied\"")
                    && l.contains(&card_key)
                    && l.contains(&peer_key)
                    && line_keyed(l, round, patch_id)
            })
        })
        .collect()
}

/// #3511 — true iff JEFF's go is recorded for this card at THIS round. The go is
/// the human accept (DEC-048), given AFTER the demoer presents the finished, gated
/// variant — recorded via `werk-demo go <card>`. The go-INPUT method (how Jeff
/// says go) is Jeff's to shape; this is the witness slot the block reads. Patch-
/// tolerant (#3461 line_keyed) like the gathers, so his go survives a content-
/// preserving rebase. NOT the peers' gathers: peer-review is never the verdict —
/// proving is Jeff's, the /demo skill's own rule ("never peer-blessing").
pub fn jeff_go_recorded(witness: &str, card: u64, round: &str, patch_id: &str) -> bool {
    let card_key = format!("\"card_id\":{},", card);
    witness.lines().any(|l| {
        l.contains("\"event\":\"demo.go\"") && l.contains(&card_key) && line_keyed(l, round, patch_id)
    })
}



pub fn gates_missing(witness: &str, card: u64, round: &str, patch_id: &str) -> Vec<&'static str> {
    let card_key = format!("\"card_id\":{},", card);
    REQUIRED_GATES
        .iter()
        .copied()
        .filter(|gate| {
            let gate_key = format!("\"gate\":\"{}\"", gate);
            !witness.lines().any(|l| {
                l.contains("\"event\":\"demo.gate.result\"")
                    && l.contains(&card_key)
                    && l.contains(&gate_key)
                    && line_keyed(l, round, patch_id)
            })
        })
        .collect()
}

/// Extract a JSON string field's value from a witness line (zero-dep): finds
/// `"key":"` and returns up to the next quote. None if absent.
fn json_str_after(line: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\":\"", key);
    let start = line.find(&pat)? + pat.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// #3284 — each required gate's recorded verdict for the card, in REQUIRED_GATES
/// order. The LAST demo.gate.result wins (a re-run gate overwrites). A gate with
/// no result is "-" (not run). Feeds the decision surface so Jeff sees WHAT each
/// gate found, not just which ran (the #3251 residual).
pub fn gate_verdicts(witness: &str, card: u64) -> Vec<(&'static str, String)> {
    let card_key = format!("\"card_id\":{},", card);
    REQUIRED_GATES
        .iter()
        .copied()
        .map(|gate| {
            let gate_key = format!("\"gate\":\"{}\"", gate);
            let result = witness
                .lines()
                .rev()
                .find(|l| {
                    l.contains("\"event\":\"demo.gate.result\"")
                        && l.contains(&card_key)
                        && l.contains(&gate_key)
                })
                .and_then(|l| json_str_after(l, "result"))
                .unwrap_or_else(|| "-".to_string());
            (gate, result)
        })
        .collect()
}

/// #3284 (AC1-4) — the pipeline execution-state cockpit. From a card's real event
/// history (the chorus-api /logs/card/:id response text, scanned zero-dep) render
/// the Half-A verb checklist + the Half-B verbs a GO triggers, so Jeff sees WHERE
/// the pipeline parked in one glance — never having to ask "which verb are we on."
/// A verb whose `.completed` event is absent shows `-` (unknown), NEVER a fake ✓
/// (AC4: honest degrade — an empty `events_text`, e.g. the API was unreachable,
/// yields all `-`). `test` is advisory: pass→✓, fail→⚠, not-run→-.
pub fn render_execution_state(events_text: &str, test_result: Option<&str>) -> String {
    let done = |ev: &str| events_text.contains(&format!("\"event\":\"{}\"", ev));
    let mark = |ok: bool| if ok { "✓" } else { "-" };
    let test_mark = match test_result {
        Some("fail") => "⚠",
        Some(_) => "✓",
        None => "-",
    };
    format!(
        "commit {}  push {}  build {}  test {}  deploy-werk {}  env-up {}  ▸ demo ◂ ⏸ HERE\n on GO → merge · sync · deploy · accept",
        mark(done("commit.completed")),
        mark(done("push.completed")),
        mark(done("build.completed")),
        test_mark,
        mark(done("deploy.completed")),
        mark(done("env.up.completed")),
    )
}

/// #3284 — best-effort fetch of a card's event history from chorus-api for the
/// cockpit. Returns the raw response body, or "" on any failure (→ honest degrade,
/// the cockpit shows all `-`). Never blocks the demo (same contract as the witness).
fn fetch_card_events(card: u64) -> String {
    run(
        "curl",
        &[
            "-s", "-f", "--max-time", "5",
            &format!("http://localhost:3340/api/chorus/logs/card/{}", card),
        ],
    )
    .unwrap_or_default()
}

/// #3284 — render the gate verdicts as a one-line summary for the decision
/// surface: `gates: product ✓  code ✓  quality ✗  arch -  ops ✓`. pass→✓,
/// fail→✗, not-run→- (honest: a gate that didn't run is never shown as passed).
pub fn render_gate_summary(witness: &str, card: u64) -> String {
    let mark = |r: &str| match r {
        "pass" => "✓",
        "fail" => "✗",
        "-" => "-",
        _ => "?",
    };
    let parts: Vec<String> = gate_verdicts(witness, card)
        .iter()
        .map(|(g, r)| format!("{} {}", g, mark(r)))
        .collect();
    format!("gates: {}", parts.join("  "))
}

/// #3284 — the REQUIRED feedback: WHAT each gate found (its findings), so the
/// announce carries real review, not just pass/fail. One line per gate that left
/// findings; "" when none did (the demo still required all 5 to RUN, via AC6).
pub fn render_gate_feedback(witness: &str, card: u64) -> String {
    let card_key = format!("\"card_id\":{},", card);
    let mut lines = Vec::new();
    for gate in REQUIRED_GATES {
        let gate_key = format!("\"gate\":\"{}\"", gate);
        if let Some(l) = witness.lines().rev().find(|l| {
            l.contains("\"event\":\"demo.gate.result\"")
                && l.contains(&card_key)
                && l.contains(&gate_key)
        }) {
            let f = json_str_after(l, "findings").unwrap_or_default();
            if !f.is_empty() && f != "-" {
                lines.push(format!("  {}: {}", gate, f));
            }
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("feedback:\n{}", lines.join("\n"))
    }
}

/// #3511 — summarize the ROLE feedback (the peer gathers) for the announce: each
/// peer's recorded demo.gather.replied verdict + substance. Distinct from the
/// per-gate findings above — this is the TEAM's review Jeff weighs before his go.
/// #4474 — only THIS round's peers: the demoer never reviews itself, and a reply
/// from an earlier round is not this round's review (run 14 showed a kade "block"
/// recorded rounds earlier, when the demo ran without a role and kade was a peer).
pub fn render_role_feedback(witness: &str, card: u64, role: &str, round: &str, patch_id: &str) -> String {
    let card_key = format!("\"card_id\":{},", card);
    let mut lines = Vec::new();
    for peer in gather_peers(role) {
        let peer_key = format!("\"peer\":\"{}\"", peer);
        if let Some(l) = witness.lines().rev().find(|l| {
            l.contains("\"event\":\"demo.gather.replied\"") && l.contains(&card_key) && l.contains(&peer_key)
                && line_keyed(l, round, patch_id)
        }) {
            let verdict = json_str_after(l, "verdict").unwrap_or_else(|| "pass".to_string());
            let note = json_str_after(l, "note").unwrap_or_default();
            let tail = if note.is_empty() { String::new() } else { format!(" — {}", note) };
            lines.push(format!("  {}: {}{}", peer, verdict, tail));
        }
    }
    if lines.is_empty() { String::new() } else { format!("team review:\n{}", lines.join("\n")) }
}

/// #3511 — the card's TITLE line from `cards view` (first non-empty line, "#N …").
pub fn card_title(card_view: &str) -> String {
    card_view.lines().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("").to_string()
}

/// #3511 — pull a "## <header>" markdown section body from `cards view`. Used for
/// the demo CLAIM (the card's Experience = what "working" looks like). Returns the
/// section's non-empty lines (until the next "## " header or EOF), space-joined.
pub fn extract_section(card_view: &str, header: &str) -> String {
    let want = format!("## {}", header);
    let mut out = Vec::new();
    let mut in_sec = false;
    for line in card_view.lines() {
        let t = line.trim();
        if t.starts_with("## ") {
            in_sec = t.eq_ignore_ascii_case(&want);
            continue;
        }
        if in_sec && !t.is_empty() {
            out.push(t.to_string());
        }
    }
    out.join(" ")
}

/// #3511 — the AC checklist from `cards view` (the `- [ ]`/`- [x]` items, each with
/// its done-box) so Jeff sees what's demonstrated vs outstanding, not just a count.
pub fn render_ac_items(card_view: &str) -> String {
    let items: Vec<String> = card_view
        .lines()
        .map(|l| l.trim())
        .filter(|t| t.starts_with("- [x]") || t.starts_with("- [ ]"))
        .map(|t| format!("  {}", t))
        .collect();
    if items.is_empty() { String::new() } else { format!("AC:\n{}", items.join("\n")) }
}

// #3331 — the #3237 Decision enum + read_decision were REMOVED here: #3279 retired
// the blocking-decision step (demo presents-and-exits; werk-land's shared-trace
// records the verdict, werk-accept byte-matches the demo.decision line format
// independently). Zero call sites confirmed semantically (ast-grep: no
// `read_decision(...)` anywhere; every `Decision::` reference was inside this
// block itself; chorus-hooks' `Decision` in types.rs is an unrelated type).

/// #3263 — the latest `demo.test_result` recorded for this card, if any. The
/// pipeline's test step records pass|fail to the witness so the informed-go
/// check can require it (and Jeff sees it before deciding). None = tests were
/// never run/recorded → "I can't show it works, approve anyway" is an
/// UN-reachable state for the decision (the VP-demo trap). Comma-terminated
/// card match, same anti-collision rule as gates_missing/read_decision.
pub fn test_result_recorded(witness: &str, card: u64) -> Option<String> {
    let card_key = format!("\"card_id\":{},", card);
    witness
        .lines()
        .rev()
        .filter(|l| l.contains("\"event\":\"demo.test_result\"") && l.contains(&card_key))
        .find_map(|l| json_str_field(l, "result"))
}

/// #3983 — the `,"coverage":"<state>:<n>"` field, or nothing when the run
/// carried none. Only the four typed states are written.
pub fn coverage_field(coverage: &str) -> String {
    let state = coverage.split(':').next().unwrap_or("");
    if ["covered", "zero-coverage", "no-code", "registry-unreadable"].contains(&state)
        && coverage.split(':').nth(1).map(|n| n.parse::<u64>().is_ok()).unwrap_or(false)
    {
        format!(",\"coverage\":\"{}\"", coverage)
    } else {
        String::new()
    }
}

/// #3983 — the coverage recorded with this card's latest test result, if any.
pub fn coverage_recorded(witness: &str, card: u64) -> Option<String> {
    let card_key = format!("\"card_id\":{},", card);
    witness
        .lines()
        .rev()
        .find(|l| l.contains("\"event\":\"demo.test_result\"") && l.contains(&card_key))
        .and_then(|l| json_str_field(l, "coverage"))
}

/// #3983 — the card's own-test evidence in words, for the badge and accept.
pub fn coverage_words(coverage: Option<&str>) -> String {
    let Some(c) = coverage else { return "own tests: not recorded".to_string() };
    let (state, n) = c.split_once(':').unwrap_or((c, "0"));
    match state {
        "covered" => format!("own tests: {} registered case(s) ran", n),
        "zero-coverage" => "own tests: ZERO-COVERAGE — no registered test of the touched code ran".to_string(),
        "no-code" => "own tests: none needed — no testable code touched".to_string(),
        "registry-unreadable" => "own tests: UNKNOWN — the tests registry did not answer".to_string(),
        other => format!("own tests: {}", other),
    }
}

/// #3638 — the `demo.test_result` recorded under THIS run's trace, if any. The
/// pipeline's BLOCKING werk-test step records its verdict (via `werk-demo
/// test-result`) under the run's shared CHORUS_TRACE_ID moments before the demo
/// step; that same-run verdict is authoritative and the demo must not overwrite
/// it with its generic self-run. Trace-scoped so a record from an EARLIER round
/// never satisfies this — cross-round freshness stays with test_result_recorded.
pub fn test_result_recorded_for_trace(witness: &str, card: u64, trace: &str) -> Option<String> {
    if trace.is_empty() {
        return None;
    }
    let card_key = format!("\"card_id\":{},", card);
    witness
        .lines()
        .rev()
        .filter(|l| l.contains("\"event\":\"demo.test_result\"") && l.contains(&card_key))
        .filter(|l| json_str_field(l, "trace_id").as_deref() == Some(trace))
        .find_map(|l| json_str_field(l, "result"))
}

/// #4012 — the `demo.test_result` recorded for THIS CONTENT, if any. Measured
/// 2026-08-25: 631 of 1375 demo.test_ran rows were self-runs and every one said
/// "fail"; 58 cards had a real pass overwritten. Cause: the lookup above is
/// TRACE-scoped, and #3955's fast path re-presents identical content under a NEW
/// trace, so the verdict already paid for that patch was invisible.
///
/// Patch-keying is strictly STRONGER than trace-keying for what #3638 was
/// protecting. #3638 wanted "a record from an earlier round must not satisfy
/// this"; what actually matters is that a record for DIFFERENT CONTENT must not
/// satisfy it. Same content across rounds is the case we want to reuse, and an
/// empty patch id matches nothing rather than everything (absence refuses).
pub fn test_result_recorded_for_patch(witness: &str, card: u64, patch_id: &str) -> Option<String> {
    if patch_id.is_empty() {
        return None;
    }
    let card_key = format!("\"card_id\":{},", card);
    witness
        .lines()
        .rev()
        .filter(|l| l.contains("\"event\":\"demo.test_result\"") && l.contains(&card_key))
        .filter(|l| extract_field(l, "patch_id") == Some(patch_id))
        .find_map(|l| json_str_field(l, "result"))
}

/// What the demo step returns to the act pipeline: a human-facing message + the
/// process exit code act gates the merge on (Decision::exit_code, or a clean
/// exit 2 for gates-missing). main() turns Err into exit 1 (real error).
pub struct DemoOutcome {
    pub message: String,
    pub exit: i32,
}

/// Whitespace-tolerant JSON string-field extractor (zero-dep). Mirrors werk-pull's
/// json_str_field — never substring-match `"key":"val"` (breaks on pretty-print).
fn json_str_field(json: &str, key: &str) -> Option<String> {
    let i = json.find(&format!("\"{}\"", key))?;
    let rest = &json[i + key.len() + 2..];
    let colon = rest.find(':')?;
    let after = &rest[colon + 1..];
    let q1 = after.find('"')?;
    let tail = &after[q1 + 1..];
    let q2 = tail.find('"')?;
    Some(tail[..q2].to_string())
}

// --- side-effecting helpers ---

/// JSONL witness — best-effort append; swallows its own errors so logging can NEVER
/// affect the operation (non-transactional, per the blueprint). Loki ingests it.
fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-demo.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let line = jsonl_line(ts, event, role, card, trace, extra);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Run a CLI, capture stdout; non-zero exit is a typed error (no silent failure).
fn run(cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{} {}: {}", cmd, args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// flock guard — auto-releases on drop (and on crash, kernel-level).
pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

pub fn lock(home: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = home.join(".git/chorus-demo.lock");
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&p)
        .map_err(|e| format!("cannot open lock {}: {}", p.display(), e))?;
    let start = Instant::now();
    loop {
        if unsafe { flock(f.as_raw_fd(), LOCK_EX_NB) } == 0 {
            return Ok(FlockGuard(f));
        }
        if start.elapsed() >= timeout {
            return Err("another demo holds the repo lock (timed out)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// #3116/#3183 — resolve a platform script ABSOLUTELY from home so spawns work
/// under ANY PATH. werk-mcp.sh's step-4.5 demo invocation (and the chorus-mcp
/// daemon) lack platform/scripts on PATH; bare `cards` died "No such file or
/// directory" the first time demo ran in the real flow. This is the #3151 fix
/// (ported to werk-accept by #3183) now applied to werk-demo's `cards` spawns.
fn script_path(home: &Path, name: &str) -> String {
    format!("{}/platform/scripts/{}", home.display(), name)
}

// --- ported from demo_preflight.rs (#1657) + preflight.sh + #2897 trace ---

/// Post the `demo:preflight-pass` card comment — the SINGLE gate-evidence
/// (#2910) that done-gate.sh / accept_gate look for at /acp time. Without
/// this, /acp refuses with "no demo evidence". This IS demo's gate output.
fn post_preflight_evidence(home: &Path, card: u64, role: &str, checked: usize, total: usize, trace: &str) -> R<()> {
    let comment = format!(
        "demo:preflight-pass ac={}/{} — {} (werk-demo, trace {})",
        checked, total, role, trace
    );
    run(&script_path(home, "cards"), &["comment", &card.to_string(), &comment])
        .map(|_| ())
        .map_err(|e| format!("post evidence: {}", e))
}

/// #2897: write the trace_id to /tmp/demo-trace-<card>.txt so chorus_log
/// auto-reads it and downstream hooks (accept_gate at /acp) propagate the
/// same trace. Best-effort — failure shouldn't block the demo (matches the
/// old demo_preflight.rs warn-and-continue behavior).
fn write_trace_file(card: u64, trace: &str) {
    let p = format!("/tmp/demo-trace-{}.txt", card);
    let _ = fs::write(&p, trace);
}

// --- ported from /demo Step 3 (smoke-check.sh) ---

// --- ported from /demo Step 5 (signal) ---

/// Emit one spine event via the canonical chorus-log subprocess (best-effort,
/// like the jsonl witness — never blocks the act). Mirrors the chorus-log
/// CLI invocation the old demo skill uses for every event.
fn emit_spine(home: &Path, event: &str, role: &str, card: u64, trace: &str) {
    if let Ok(p) = path(&home.join("platform/scripts/chorus-log")) {
        let _ = run(
            "bash",
            &[
                p,
                event,
                role,
                &format!("card={}", card),
                &format!("trace={}", trace),
            ],
        );
    }
}

/// #3281 — emit a refusal to the spine WITH a `reason=` field, so an agent-side
/// block becomes countable on the pain board. Best-effort, like emit_spine (a
/// spine-emit failure never blocks the act). Separate from emit_spine so its 6
/// existing callers are untouched; args built by the pure `refuse_spine_args`.
fn emit_spine_reason(home: &Path, event: &str, role: &str, card: u64, trace: &str, reason: &str) {
    if let Ok(p) = path(&home.join("platform/scripts/chorus-log")) {
        let owned = refuse_spine_args(event, role, card, trace, reason);
        let mut argv: Vec<&str> = Vec::with_capacity(owned.len() + 1);
        argv.push(p);
        argv.extend(owned.iter().map(|s| s.as_str()));
        let _ = run("bash", &argv);
    }
}

/// #4502 — the demo's one message, through the same Rust path the
/// clearing-nudge tool runs (`chorus-hook-shim nudge-send`, #4515): word cap,
/// recipient check, spine, pulse. It is a NOTICE (expects none): the demo asks
/// Jeff for his go, it does not make the builder owe a reply. The gather that
/// needed `expects reply` (#3443) is gone — peer reviews are headless steps —
/// and a reply-required announce made the builder answer it, one more message.
pub fn nudge_send_args(from: &str, to: &str, message: &str) -> Vec<String> {
    // #4515 — the old MCP server is gone; nudges go through the same Rust path
    // the clearing-nudge tool runs (`chorus-hook-shim nudge-send`):
    // word cap, recipient check, spine, pulse. The message is one argv item,
    // never spliced into JSON.
    vec!["nudge-send".into(), from.into(), to.into(), message.into(), "--expects".into(), "none".into()]
}

/// #4515 — the nudge binary. Tests point CHORUS_HOOK_SHIM at a dead path so a
/// test never reaches live pulse (#3995's rule, same as CHORUS_BRIDGE_URL).
fn hook_shim() -> String {
    std::env::var("CHORUS_HOOK_SHIM").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "chorus-hook-shim".to_string())
}


/// #3479 — record a peer review's verdict (pass|concerns|block|error) and
/// substance on the witness. #4502 — the single writer, called by the headless
/// review step (run_one_review); keyed peer+round+patch_id so a review survives
/// rebase-churn (#3461 line_keyed). The announce step reads it.
pub fn record_gather_replied(home: &Path, role: &str, card: u64, peer: &str, verdict: &str, note: &str) {
    let round = current_round(role, card);
    let pid = current_patch_id(role, card);
    let trace = run_trace();
    let n = note.replace('\\', " ").replace('"', "'").replace('\n', " ");
    jsonl(home, role, card, &trace, "demo.gather.replied",
          &format!(",\"peer\":\"{}\",\"round\":\"{}\",\"patch_id\":\"{}\",\"verdict\":\"{}\",\"note\":\"{}\"",
                   peer, round, pid, verdict, n));
    emit_spine(home, "demo.gather.replied", role, card, &trace);
}



/// #4075 — the werk's own Clearing (env-up runs it per role, see werk-deploy
/// demo_env env_services): silas=3481, kade=3482, wren=3483. The announce
/// carries it so Jeff opens THIS round's room, not prod's.
pub fn clearing_url_for(role: &str) -> String {
    format!("http://localhost:{}/", variant_port(role, 3481, 3470))
}

/// #4625 — a card that changes Clearing v2 gets its own read-only v2 in demo
/// (werk-deploy demo_env: base 3501, same role slots), and its link goes there.
const CLEARING_V2_SRC: &str = "directing/clearing-v2/";
const CLEARING_V2_BASE: u16 = 3501;

pub fn is_clearing_v2_card(changed: &[String]) -> bool {
    changed.iter().any(|f| f.starts_with(CLEARING_V2_SRC))
}

pub fn clearing_url_for_card(role: &str, changed: &[String]) -> String {
    if is_clearing_v2_card(changed) {
        format!("http://localhost:{}/", variant_port(role, CLEARING_V2_BASE, 3472))
    } else {
        clearing_url_for(role)
    }
}

/// #4625 — why a link cannot show this card: a v2 card whose link isn't a v2 demo port.
pub fn clearing_link_refusal(changed: &[String], url: &str) -> Option<String> {
    if !is_clearing_v2_card(changed) {
        return None;
    }
    let v2 = (0..4u16).any(|slot| url.contains(&format!(":{}/", CLEARING_V2_BASE + slot)));
    if v2 { None } else { Some(format!("this card changes Clearing v2, but its demo link {} is not a Clearing v2 demo", url)) }
}

/// #4458 — a role's demo-copy port: the service's first-role port plus the
/// role's slot, in the order werk-deploy's demo_env::known_roles() gives them
/// (this crate takes no dependency on werk-deploy, ADR-032). A role with no
/// slot gets the production port, as before.
pub fn variant_port(role: &str, base: u16, prod: u16) -> u16 {
    match known_agent_roles().iter().position(|r| *r == role) {
        Some(slot) => base + slot as u16,
        None => prod,
    }
}

/// #4502 — the ONE message a demo round sends (Jeff 10-18 11:33: "demo is
/// really noisy and repetitive about messages it sends as nudges"; folds #3971).
/// Measured 10-18 in this file: each announce sent 5 — four room posts (the
/// "[demo] presenting" line, "[TEST SURFACE READY]", "[DEMO READY FOR JEFF]",
/// the one-line decision surface) plus this nudge. Now only this nudge goes
/// out: card, round, the gate/review/test/AC tally, where to look, the go.
/// Sent from "system" to the demoer's session (#3544: from≠to so it injects).
pub fn announce_line(card: u64, round: &str, tally: &str, variant_url: &str, clearing: &str) -> String {
    let url = if variant_url.is_empty() { "(variant up)" } else { variant_url };
    format!("Demo ready — #{} · round {} · {} · {} · clearing {} · go / no / more", card, round, tally, url, clearing)
}

/// #4710 — where Jeff signs this run's go with his phone. The run id is the
/// cicd run's (the demo gets it as TRACE); the page asks the server for the round.
pub fn go_link(card: u64, run: &str, role: &str) -> String {
    let base = std::env::var("CHORUS_GO_ORIGIN").ok().filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://lightlifeurbangardens.com".to_string());
    format!("{base}/go/?card={card}&run={run}&role={role}")
}

/// #4502 — true when this round already sent its one ask, or Jeff's go is in:
/// either way the announce sends nothing (a re-present of the same round, or a
/// retry after the go, is silent). Pure; keyed like every other witness read.
pub fn announce_silent(witness: &str, card: u64, round: &str, patch_id: &str) -> bool {
    let card_key = format!("\"card_id\":{},", card);
    jeff_go_recorded(witness, card, round, patch_id)
        || witness.lines().any(|l| {
            l.contains("\"event\":\"demo.announced\"") && l.contains(&card_key) && line_keyed(l, round, patch_id)
        })
}

fn announce_to_jeff(from: &str, trace: &str, msg: &str) {
    let args = nudge_send_args("system", from, msg);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    let _ = run(&hook_shim(), &argv);
    let _ = trace;
}

/// Step 5: signal — cards demo + spine event. Best-effort: the
/// announce, not a gate. #4502 — the feedback-nudge loop is gone: peer reviews
/// are their own headless steps (#3539), nobody is asked to reply.
fn signal(card: u64, role: &str, home: &Path, trace: &str) {
    let card_s = card.to_string();
    // board demo signal
    let _ = run(&script_path(home, "cards"), &["demo", &card_s]);
    emit_spine(home, "card.demo.started", role, card, trace);

    // #4502 — no room post here: the round sends one message, at the end of announce
}

// --- session-smoke (#4502) ---
//
// AC: "The demo starts a throwaway session of each changed role from the werk
// build (its own config and hooks), signs it in, and runs one tool call, before
// Jeff's go." Silas 10-14: #4377, #4520 and #4444 were proven only after land.

/// Session config every Claude role loads: a change here touches all of them.
const SMOKE_SHARED: [&str; 4] = ["designing/claudemd/shared/", ".claude/", "platform/services/chorus-hooks/", "skills/"];
/// The Claude-session roles (abby-normal runs on agy, not claude).
const SMOKE_CLAUDE_ROLES: [&str; 3] = ["wren", "silas", "kade"];

/// #4502 — the roles whose session config the card's changed paths touch:
/// roles/<r>/ or designing/claudemd/roles/<r>/ for that role, the shared
/// paths above for every Claude role. Order: PEER_ROLES, then PROVER_ROLES
/// (#4691: borg-queen's files are named too, never skipped). Pure.
pub fn smoke_roles(changed: &[String]) -> Vec<&'static str> {
    let shared = changed.iter().any(|f| SMOKE_SHARED.iter().any(|p| f.starts_with(p)));
    PEER_ROLES
        .into_iter()
        .chain(PROVER_ROLES)
        .filter(|r| {
            (shared && SMOKE_CLAUDE_ROLES.contains(r))
                || changed.iter().any(|f| {
                    f.starts_with(&format!("roles/{}/", r)) || f.starts_with(&format!("designing/claudemd/roles/{}/", r))
                })
        })
        .collect()
}

/// The card's changed paths (git diff --name-only origin/main...HEAD), trusting
/// only the card's own werk (#4599/#4602: a werk another user owns is "dubious
/// ownership" to git otherwise).
pub fn changed_paths(werk: &str) -> Result<Vec<String>, String> {
    let o = Command::new("git")
        .env("GIT_CONFIG_COUNT", "1").env("GIT_CONFIG_KEY_0", "safe.directory").env("GIT_CONFIG_VALUE_0", werk)
        .arg("-C").arg(werk).args(["diff", "--name-only", "origin/main...HEAD"]).output()
        .map_err(|e| format!("git could not run in {}: {}", werk, e))?;
    if !o.status.success() {
        return Err(format!("git diff failed in {}: {}", werk, String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&o.stdout).lines().map(str::to_string).collect())
}

/// One throwaway headless session of `r`, started in the werk's roles/<r>/ so it
/// loads that build's own .claude settings and hooks, asked for exactly one
/// Bash call. Ok when the call's output comes back; Err names why not.
fn smoke_one(werk: &str, r: &str, card: u64) -> Result<(), String> {
    let bin = claude_bin().ok_or("claude binary not resolvable")?;
    let dir = Path::new(werk).join("roles").join(r);
    if !dir.is_dir() {
        return Err(format!("no roles/{} in the werk", r));
    }
    // the reply must carry a value only the tool call can read: a nonce in a
    // file, so a model that answers without running Bash cannot pass
    let nonce = format!("smoke-{}-{}-{}", card, r, SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let nonce_file = env::temp_dir().join(format!("werk-demo-smoke-{}-{}-{}", card, r, std::process::id()));
    fs::write(&nonce_file, &nonce).map_err(|e| format!("cannot write the smoke nonce: {}", e))?;
    let cmd = format!("cat {}", nonce_file.display());
    let token = nonce;
    let model = env::var("CHORUS_GATE_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
    let mut child = Command::new(&bin)
        .current_dir(&dir)
        .env("CHORUS_ROLE", r).env("ROLE", r).env("CHORUS_HEADLESS", "1")
        .args(["-p", "--model", &model, "--no-session-persistence", "--output-format", "text",
               "--allowedTools", &format!("Bash({})", cmd)])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("claude spawn failed: {}", e))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("Run exactly one Bash command, `{}`, and reply with its output only.", cmd).as_bytes());
    }
    let secs: u64 = env::var("CHORUS_SMOKE_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(180);
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) if start.elapsed() >= Duration::from_secs(secs) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("no answer in {}s", secs));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => return Err(format!("wait failed: {}", e)),
        }
    };
    let _ = fs::remove_file(&nonce_file);
    let mut out = String::new();
    let mut err = String::new();
    if let Some(mut o) = child.stdout.take() { let _ = o.read_to_string(&mut out); }
    if let Some(mut e) = child.stderr.take() { let _ = e.read_to_string(&mut err); }
    if !status.success() {
        return Err(format!("session exited {}: {}", status.code().unwrap_or(-1), err.trim().lines().last().unwrap_or("")));
    }
    if !out.contains(&token) {
        return Err(format!("the tool call did not come back (no `{}` in the reply)", token));
    }
    Ok(())
}

/// #4502 — the session-smoke step. No role config in the diff is a named
/// pass, never a silent one; any role that cannot start, run its hooks and
/// make one tool call fails the step, named.
pub fn session_smoke_step(card: u64, role: &str, home: &Path) -> R<DemoOutcome> {
    let werk_base = env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
        format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
    });
    let werk = format!("{}/{}-{}", werk_base, role, card);
    let trace = run_trace();
    let changed = match changed_paths(&werk) {
        Ok(c) => c,
        Err(why) => return Ok(DemoOutcome { message: format!("session-smoke #{} could not read the diff: {}", card, why), exit: 1 }),
    };
    let roles = smoke_roles(&changed);
    if roles.is_empty() {
        jsonl(home, role, card, &trace, "demo.smoke.result", ",\"result\":\"none\"");
        return Ok(DemoOutcome { message: format!("session-smoke #{}: nothing to smoke: no role config in the diff", card), exit: 0 });
    }
    let mut failed = Vec::new();
    let mut by_hand = Vec::new();
    for r in &roles {
        // #4691 (Jeff 2026-10-22 16:12, "2"): an agy role is not smoked here (no
        // headless agy smoke on this box); it is named, recorded by-hand, and that
        // role checks its own session by hand after the land. Never a pass.
        if !SMOKE_CLAUDE_ROLES.contains(r) {
            jsonl(home, role, card, &trace, "demo.smoke.result",
                  &format!(",\"smoked\":\"{}\",\"result\":\"by-hand\",\"why\":\"runs on agy: not smoked here\"", r));
            by_hand.push(format!("{}: runs on agy, not smoked here; {} checks the session by hand after the land", r, r));
            continue;
        }
        let res = smoke_one(&werk, r, card);
        let (word, why) = match &res { Ok(()) => ("pass", String::new()), Err(e) => ("fail", e.clone()) };
        jsonl(home, role, card, &trace, "demo.smoke.result",
              &format!(",\"smoked\":\"{}\",\"result\":\"{}\",\"why\":\"{}\"", r, word, why.replace('"', "'")));
        if let Err(e) = res { failed.push(format!("{}: {}", r, e)); }
    }
    emit_spine(home, "demo.smoke.recorded", role, card, &trace);
    let smoked: Vec<&str> = roles.iter().copied().filter(|r| SMOKE_CLAUDE_ROLES.contains(r)).collect();
    let mut parts = Vec::new();
    if !smoked.is_empty() { parts.push(format!("{} started from the werk build and made one tool call", smoked.join(", "))); }
    parts.extend(by_hand);
    if failed.is_empty() {
        Ok(DemoOutcome { message: format!("session-smoke #{}: {}", card, parts.join(" · ")), exit: 0 })
    } else {
        Ok(DemoOutcome { message: format!("session-smoke #{} failed — {}", card, failed.join(" · ")), exit: 1 })
    }
}

// --- the demo steps (#4502) ---
//
// #4502 (Jeff 2026-10-18 11:31, "that looks right") — the demo is dagu steps,
// not one binary run in a hidden order:
//   validate → [gate-product, -code, -quality, -arch, -ops, review-peer-1, -2]
//   in parallel → session-smoke → announce → Jeff's go.
// Each step is one subcommand below; dagu shows which one stopped and retries
// it alone. The old all-in-one `werk-demo <card> <role>` (gates one by one,
// then a poll for peer reviews, then the announce) is gone: the 2 peer reviews
// are headless runs (#3539), so they are parallel steps, not a wait.
// Gates and reviews inform; Jeff's go stays sovereign (#3263/DEC-048).

fn read_witness(home: &Path) -> String {
    fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default()
}

/// #4502 — the latest recorded (result, text) for one keyed step in this
/// round: a gate (`demo.gate.result`, keyed `gate`, fields result/findings) or
/// a peer review (`demo.gather.replied`, keyed `peer`, fields verdict/note).
/// None when nothing is recorded for this round/patch. Pure.
#[allow(clippy::too_many_arguments)]
pub fn step_result(
    witness: &str,
    card: u64,
    event: &str,
    key: &str,
    value: &str,
    result_field: &str,
    text_field: &str,
    round: &str,
    patch_id: &str,
) -> Option<(String, String)> {
    let card_key = format!("\"card_id\":{},", card);
    let event_key = format!("\"event\":\"{}\"", event);
    let value_key = format!("\"{}\":\"{}\"", key, value);
    witness
        .lines()
        .rev()
        .find(|l| l.contains(&event_key) && l.contains(&card_key) && l.contains(&value_key) && line_keyed(l, round, patch_id))
        .map(|l| {
            (
                json_str_after(l, result_field).unwrap_or_default(),
                json_str_after(l, text_field).unwrap_or_default(),
            )
        })
}

/// #4502 — a step's exit from what it recorded. A result that says the step
/// could not run ("error", or nothing recorded) is non-zero and names why; any
/// verdict (pass, fail, concerns, block) is exit 0 — the gate informs, Jeff's
/// go decides (#3263). Pure.
pub fn step_outcome(what: &str, card: u64, recorded: Option<(String, String)>) -> DemoOutcome {
    match recorded {
        None => DemoOutcome {
            message: format!("{} for #{} could not run: no result recorded", what, card),
            exit: 1,
        },
        Some((r, why)) if r == "error" => DemoOutcome {
            message: format!(
                "{} for #{} could not run: {}",
                what,
                card,
                if why.is_empty() { "no reason recorded" } else { why.as_str() }
            ),
            exit: 1,
        },
        Some((r, why)) => DemoOutcome {
            message: format!("{} for #{}: {}{}", what, card, r, if why.is_empty() { String::new() } else { format!(" — {}", why) }),
            exit: 0,
        },
    }
}

/// #4502 step 1 — validate: the card exists, is WIP/Now and has AC. Records the
/// AC count (informational: the announce shows it, it never refuses, #3263) and
/// the single gate-evidence comment + trace file /acp reads (#2910 / #2897).
pub fn validate_step(card: u64, role: &str, home: &Path) -> R<DemoOutcome> {
    let trace = run_trace();
    jsonl(home, role, card, &trace, "demo.started", "");
    let card_s = card.to_string();
    let cj = run(&script_path(home, "cards"), &["view", &card_s, "--json"])
        .map_err(|e| format!("validate: cannot read card #{}: {}", card, e))?;
    let status = json_str_field(&cj, "status").unwrap_or_default();
    if status != "WIP" && status != "Now" {
        jsonl(home, role, card, &trace, "demo.refused", &fail_extra("wrong-status"));
        emit_spine_reason(home, "demo.refused", role, card, &trace, "wrong-status");
        return Err(format!("#{} is {} — must be WIP/Now to demo", card, status));
    }
    let cv = run(&script_path(home, "cards"), &["view", &card_s])?;
    let (checked, total) = ac_counts(&cv);
    if total == 0 {
        jsonl(home, role, card, &trace, "demo.refused", &fail_extra("no-ac"));
        emit_spine_reason(home, "demo.refused", role, card, &trace, "no-ac");
        return Err(format!("#{} has no acceptance criteria", card));
    }
    jsonl(home, role, card, &trace, "demo.ac_status",
          &format!(",\"checked\":{},\"total\":{}", checked, total));
    post_preflight_evidence(home, card, role, checked, total, &trace)?;
    write_trace_file(card, &trace);
    jsonl(home, role, card, &trace, "demo.preflight.passed", &format!(",\"ac\":\"{}/{}\"", checked, total));
    Ok(DemoOutcome {
        message: format!("#{} valid: {}, AC {}/{} checked (shown at announce, never a refusal)", card, status, checked, total),
        exit: 0,
    })
}

/// #4502 — run ONE gate (product|code|quality|arch|ops) as its own dagu step,
/// unless it already has a result for this round (a retry after a pass is a
/// no-op). Exit 0 when a verdict is recorded; non-zero, named, when the gate
/// could not run (no headless model on the box, or an "error" result).
pub fn run_gate_step(card: u64, role: &str, home: &Path, gate: &str) -> R<DemoOutcome> {
    if !REQUIRED_GATES.contains(&gate) {
        return Err(format!("unknown gate '{}' — one of {:?}", gate, REQUIRED_GATES));
    }
    let round = current_round(role, card);
    let patch = current_patch_id(role, card);
    let what = format!("gate {}", gate);
    let recorded = |w: &str| step_result(w, card, "demo.gate.result", "gate", gate, "result", "findings", &round, &patch);
    if let Some(done) = recorded(&read_witness(home)).filter(|(r, _)| r != "error") {
        return Ok(step_outcome(&format!("{} (already recorded this round)", what), card, Some(done)));
    }
    if !headless_available() {
        return Ok(DemoOutcome {
            message: format!("{} for #{} could not run: no headless model on this box (no claude, no chorus-agent profile)", what, card),
            exit: 1,
        });
    }
    let trace = run_trace();
    emit_spine(home, "demo.gates.running", role, card, &trace);
    run_one_gate(home, role, card, gate, &round);
    Ok(step_outcome(&what, card, recorded(&read_witness(home))))
}

/// #4502 — run the n-th peer review (1-based, the order gather_peers gives) as
/// its own dagu step: one headless review loaded with that peer's CLAUDE.md
/// (#3539). Same exit rule as a gate. No longer waits for the gates: the
/// reviews run beside them.
pub fn run_review_step(card: u64, role: &str, home: &Path, n: usize) -> R<DemoOutcome> {
    let peers = gather_peers(role);
    let peer = match n.checked_sub(1).and_then(|i| peers.get(i)) {
        Some(p) => *p,
        None => return Err(format!("no peer {} for {} — {} has {} peers: {:?}", n, role, role, peers.len(), peers)),
    };
    let round = current_round(role, card);
    let patch = current_patch_id(role, card);
    let what = format!("review by {}", peer);
    let recorded = |w: &str| step_result(w, card, "demo.gather.replied", "peer", peer, "verdict", "note", &round, &patch);
    if let Some(done) = recorded(&read_witness(home)).filter(|(r, _)| r != "error") {
        return Ok(step_outcome(&format!("{} (already recorded this round)", what), card, Some(done)));
    }
    if !headless_available() {
        return Ok(DemoOutcome {
            message: format!("{} for #{} could not run: no headless model on this box (no claude, no chorus-agent profile)", what, card),
            exit: 1,
        });
    }
    let trace = run_trace();
    emit_spine(home, "demo.reviews.running", role, card, &trace);
    run_one_review(home, role, card, peer, &round);
    Ok(step_outcome(&what, card, recorded(&read_witness(home))))
}

/// #4502 — what announce still needs: the gates and peer reviews with no
/// usable result this round ("error" counts as missing — it never ran). Pure.
pub fn announce_missing(witness: &str, card: u64, role: &str, round: &str, patch_id: &str) -> Vec<String> {
    let usable = |r: Option<(String, String)>| r.is_some_and(|(v, _)| v != "error");
    let mut missing: Vec<String> = REQUIRED_GATES
        .iter()
        .filter(|g| !usable(step_result(witness, card, "demo.gate.result", "gate", g, "result", "findings", round, patch_id)))
        .map(|g| format!("gate-{}", g))
        .collect();
    missing.extend(
        gather_peers(role)
            .into_iter()
            .filter(|p| !usable(step_result(witness, card, "demo.gather.replied", "peer", p, "verdict", "note", round, patch_id)))
            .map(|p| format!("review-{}", p)),
    );
    missing
}

/// #4502 — the announce step: refuses, naming each missing step, unless every
/// gate and peer review has a result this round; then shows Jeff the decision
/// surface (tests verdict, AC x/y, gates, reviews, the claim) and exits 0.
/// Nothing waits or polls here.
pub fn announce_step(card: u64, role: &str, home: &Path) -> R<DemoOutcome> {
    let trace = run_trace();
    let round = current_round(role, card);
    let patch = current_patch_id(role, card);
    let missing = announce_missing(&read_witness(home), card, role, &round, &patch);
    if !missing.is_empty() {
        jsonl(home, role, card, &trace, "demo.refused",
              &format!("{},\"missing\":\"{}\"", fail_extra("gates-missing"), missing.join(",")));
        emit_spine_reason(home, "demo.refused", role, card, &trace, "gates-missing");
        return Ok(DemoOutcome {
            message: format!(
                "announce #{} refused — no result this round from: [{}]. Retry those steps; \
                 gates and reviews inform, your go decides.",
                card, missing.join(", ")
            ),
            exit: 1,
        });
    }
    let cv = run(&script_path(home, "cards"), &["view", &card.to_string()])?;
    let (checked, total) = ac_counts(&cv);

    // #3263 / #3638 / #4012 — the tests verdict on the surface: this patch's
    // werk-test result wins, then this trace's; only with neither does the
    // demo run the card's tests itself. Shown, never a refusal.
    if !std::env::var("CHORUS_DEMO_SKIP_TEST_RUN").map(|v| v == "1").unwrap_or(false) {
        let witness_now = read_witness(home);
        let paid = test_result_recorded_for_patch(&witness_now, card, &patch)
            .or_else(|| test_result_recorded_for_trace(&witness_now, card, &trace));
        if let Some(res) = paid {
            jsonl(home, role, card, &trace, "demo.test_ran",
                  &format!(",\"result\":\"{}\",\"source\":\"werk-test\"", res));
        } else {
            let werk_base = env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
                format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
            });
            let werk = format!("{}/{}-{}", werk_base, role, card);
            let res = card_test_verdict(&werk);
            record_test_result(home, role, card, res, "");
            jsonl(home, role, card, &trace, "demo.test_ran", &format!(",\"result\":\"{}\"", res));
        }
    }
    emit_spine(home, "demo.gates.recorded", role, card, &trace);

    // Step 5: signal — board demo + spine event (best-effort; it does not gate).
    signal(card, role, home, &trace);
    jsonl(home, role, card, &trace, "demo.signal.completed", "");

    // #3100 — announce the TEST SURFACE: the service ports and CLI verbs the
    // card's new code runs on, so the window is a real test window.
    // #4502 — the test surface is named in the round's one message, not posted to the room
    emit_spine(home, "demo.test_surface.ready", role, card, &trace);
    jsonl(home, role, card, &trace, "demo.test_surface.ready", "");

    // #3100 AC#4 — the visible announce, per-role variant ports (#3092).
    let variant_url = format!("http://localhost:{}/api/chorus/health", variant_port(role, 3343, 3340));
    // #4625 — a v2 card links to its own v2, and is refused if that v2 isn't up.
    let werk = format!("{}/{}-{}", env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
        format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
    }), role, card);
    let changed = changed_paths(&werk).unwrap_or_default();
    let clearing_url = clearing_url_for_card(role, &changed);
    let v2_problem = clearing_link_refusal(&changed, &clearing_url).or_else(|| {
        let up = run("curl", &["-s", "-f", "-o", "/dev/null", "--max-time", "5", &format!("{}health", clearing_url)]).is_ok();
        (is_clearing_v2_card(&changed) && !up).then(|| format!("this card changes Clearing v2, but its demo v2 {} does not answer", clearing_url))
    });
    if let Some(why) = v2_problem {
        jsonl(home, role, card, &trace, "demo.refused", &fail_extra("clearing-v2-not-shown"));
        emit_spine_reason(home, "demo.refused", role, card, &trace, "clearing-v2-not-shown");
        return Err(why);
    }
    jsonl(home, role, card, &trace, "demo.ready_for_review", "");

    // #3263 — the machine shows, it does not gate: the status below is for the
    // decision surface only.
    let witness = read_witness(home);
    let skip_variant_check =
        std::env::var("CHORUS_DEMO_SKIP_VARIANT_CHECK").map(|v| v == "1").unwrap_or(false);
    let variant_status = if skip_variant_check {
        format!("running → {}", variant_url)
    } else if run("curl", &["-s", "-f", "-o", "/dev/null", "--max-time", "5", &variant_url]).is_ok() {
        format!("running → {} (reachable)", variant_url)
    } else {
        format!("running → {} (NOT reachable)", variant_url)
    };
    let test_res = test_result_recorded(&witness, card);
    let test_summary = match &test_res {
        Some(r) => format!("tests: {} · {}", r, coverage_words(coverage_recorded(&witness, card).as_deref())),
        None => "tests: NOT run".to_string(),
    };
    let gate_summary = render_gate_summary(&witness, card);
    let gates_green = !gate_summary.contains('✗') && !gate_summary.to_lowercase().contains("fail");
    let events = fetch_card_events(card);
    let cockpit = render_execution_state(&events, test_res.as_deref());
    let gate_feedback = render_gate_feedback(&witness, card);

    // #3511 — the announce, the four things Jeff named: each gate's status, the
    // team's review, the card + AC, and a claim of how to prove it works.
    let title = card_title(&cv);
    let experience = extract_section(&cv, "Experience");
    let ac_items = render_ac_items(&cv);
    let role_feedback = render_role_feedback(&witness, card, role, &round, &patch);
    let claim = if experience.is_empty() {
        format!("🔬 To prove it works: exercise the AC against the live variant ({}).", variant_url)
    } else {
        format!("🔬 Claim — what working looks like (prove it against the variant): {}", experience)
    };

    let mut parts: Vec<String> = Vec::new();
    parts.push("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string());
    parts.push(format!("🎬 {}", if title.is_empty() { format!("#{}", card) } else { title.clone() }));
    parts.push(format!("   DEMO · ready for your GO · AC {}/{}", checked, total));
    parts.push("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".to_string());
    parts.push(cockpit);
    if !ac_items.is_empty() { parts.push(ac_items); }
    parts.push(gate_summary);
    if !gate_feedback.is_empty() { parts.push(gate_feedback); }
    if !role_feedback.is_empty() { parts.push(role_feedback); }
    parts.push(claim);
    parts.push(format!("variant: {}", variant_status));
    parts.push(test_summary.clone());
    parts.push("──────────────────────────────────".to_string());
    parts.push("Look at the variant, ask me anything, or tell me to TEST it.".to_string());
    parts.push(format!("Then your call → `werk-demo go {}` to land · no/more to hold.", card));
    let announce = parts.join("\n");
    // #3858 — the room gets ONE line; the full surface is this step's message.
    jsonl(home, role, card, &trace, "demo.decision_surface",
          &format!(",\"ac\":\"{}/{}\"", checked, total));

    // #3279 / #3459 — present-and-exit; the land half reads demo.presented and
    // Jeff's go (`werk-demo go`) from the witness, patch-keyed.
    let land_patch = std::env::var("CHORUS_WERK_BASE").ok()
        .map(|wb| patch_id_for_card(&wb, role, card))
        .unwrap_or_default();
    jsonl(home, role, card, &trace, "demo.presented",
          &format!(",\"ac\":\"{}/{}\",\"round\":\"{}\",\"variant\":\"{}\",\"patch_id\":\"{}\"",
                   checked, total, round, variant_url, land_patch));
    emit_spine(home, "demo.presented", role, card, &trace);
    // #3548 — never re-announce "ready for GO" once Jeff's go is recorded.
    // #4502 — ONE ask per round: never a second for the same round, never
    // after the go (#3971's "one card, one go"). The tally is what Jeff weighs.
    if !announce_silent(&witness, card, &round, &patch) {
        let reviews = gather_peers(role).len();
        let reviews_in = reviews - announce_missing(&witness, card, role, &round, &patch)
            .iter().filter(|m| m.starts_with("review-")).count();
        let tally = format!("gates {} · reviews {}/{} · {} · AC {}/{}",
            if gates_green { "✓" } else { "✗" }, reviews_in, reviews,
            test_summary.replace("tests: ", "tests "), checked, total);
        announce_to_jeff(role, &trace, &format!("{} · sign: {}",
            announce_line(card, &round, &tally, &variant_url, &clearing_url), go_link(card, &trace, role)));
        jsonl(home, role, card, &trace, "demo.announced",
              &format!(",\"round\":\"{}\",\"patch_id\":\"{}\"", round, patch));
    }
    Ok(DemoOutcome { message: announce, exit: 0 })
}

/// #3237 — record one gate's result into the witness so the verdict step can
/// verify the gate gather ran. Called by the /demo skill's gate subagents via
/// `werk-demo gate <card> <gate> <result>`. The witness is the same
/// ops/logs/werk-demo.jsonl the verdict + werk-accept read — one evidence file.
fn record_gate(home: &Path, role: &str, card: u64, gate: &str, result: &str, findings: &str, round: &str) {
    let trace = run_trace();
    // #3284 — feedback is REQUIRED: a gate carries WHAT it found, not just pass/fail,
    // so the announce shows real feedback. Sanitized for the one-line JSONL witness.
    let f = findings.replace('\\', " ").replace('"', "'").replace('\n', " ");
    let pid = current_patch_id(role, card); // #3461 — record patch-id so the result survives a rebase-churn
    jsonl(home, role, card, &trace, "demo.gate.result",
          &format!(",\"gate\":\"{}\",\"result\":\"{}\",\"round\":\"{}\",\"patch_id\":\"{}\",\"findings\":\"{}\"", gate, result, round, pid, f));
    emit_spine(home, "demo.gate.result", role, card, &trace);
}

// === AC1 (#3443) — werk-demo RUNS the 5 gates itself via headless `claude -p`,
// instead of depending on the /demo skill's demoer agent to spawn them. This
// retires the "a zero-dep binary can't run an LLM gate" excuse (#3116/#3318):
// `claude -p` is a subprocess like git/cargo/cards. One headless claude per
// absent gate, each fed the gate's SKILL.md as system prompt + the card AC and
// branch diff as context, emits {"result":"pass|fail","findings":"..."} which we
// record via the existing record_gate witness path. A garbled or errored gate is
// recorded as "error" (VISIBLE), never silently passed. ===

/// Resolve the `claude` CLI to an invocable path. #3443 LIVE FIX: a bare
/// `Command::new("claude")` only searches PATH — and the `act` pipeline runs
/// host-native with a PATH that does NOT include `~/.local/bin` (where claude
/// actually lives). The first live `/cw` run degraded for exactly this reason:
/// claude was there, just not on the job's PATH. So resolve it explicitly:
///   1. `CHORUS_CLAUDE_BIN` override (lets the env name the exact binary),
///   2. bare `claude` if PATH already resolves it (interactive shells),
///   3. known install locations a stripped job PATH commonly misses.
/// None = genuinely absent (hosted CI) → degrade, don't break (#3284).
fn claude_bin() -> Option<String> {
    if let Ok(p) = env::var("CHORUS_CLAUDE_BIN") {
        if !p.is_empty() && Path::new(&p).exists() {
            return Some(p);
        }
    }
    if Command::new("claude").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        return Some("claude".to_string());
    }
    let home = env::var("HOME").unwrap_or_default();
    for cand in [
        format!("{}/.local/bin/claude", home),
        format!("{}/.claude/local/claude", home),
        "/usr/local/bin/claude".to_string(),
        "/opt/homebrew/bin/claude".to_string(),
    ] {
        if Path::new(&cand).exists() {
            return Some(cand);
        }
    }
    None
}

/// Is the `claude` CLI resolvable at all? Gates can only self-run where a real
/// claude binary exists (Jeff's machine, local `act`). On hosted CI (no auth,
/// no binary) it is absent — we degrade rather than break the build (#3284).
fn claude_available() -> bool {
    claude_bin().is_some()
}

/// #4424 — gates self-run on a non-Claude model only through chorus-agent, and
/// only when CHORUS_GATE_PROFILE names a profile; unset, this is claude_available.
fn headless_available() -> bool {
    match gate_profile() {
        Some(_) => Path::new(&agent_bin()).is_file(),
        None => claude_available(),
    }
}

/// Unescape the JSON string-escapes claude emits (\" \\ \/ \n \r \t).
pub fn json_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Extract a JSON string field's RAW (still-escaped) body by name: scan for
/// `"<field>"`, the colon, the opening quote, then read to the next UNescaped
/// quote. Returns None if absent or non-string. Pure + unit-tested.
pub fn extract_json_str(json: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\"", field);
    let mut search_from = 0;
    loop {
        let hit = json[search_from..].find(&needle)? + search_from;
        let after = hit + needle.len();
        search_from = after; // advance so a non-key match doesn't loop forever
        // It's only a KEY if the next non-whitespace char is ':' — this skips a
        // bare value like `"type":"result"` matching field="result".
        let tail = json[after..].trim_start();
        if !tail.starts_with(':') {
            continue;
        }
        let after_colon = &tail[1..];
        let val = after_colon.trim_start();
        if !val.starts_with('"') {
            // non-string value for this key; not what we extract
            continue;
        }
        let body = &val[1..];
        let bytes = body.as_bytes();
        let mut i = 0;
        let mut esc = false;
        while i < bytes.len() {
            let b = bytes[i];
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                return Some(body[..i].to_string());
            }
            i += 1;
        }
        return None;
    }
}

/// Parse a gate verdict out of a headless claude gate run. The gate is prompted
/// to emit `{"result":"pass"|"fail","findings":"<one line>"}`. Handles both a
/// raw payload and the `--output-format json` envelope (whose `result` field
/// holds the assistant text). Unknown/garbled → ("error", reason) so a broken
/// gate is recorded VISIBLE, never silently passed. Pure + unit-tested.
pub fn parse_gate_verdict(stdout: &str) -> (String, String) {
    // If an envelope `result` holds nested gate JSON, parse that; else parse raw.
    let text = match extract_json_str(stdout, "result") {
        Some(r) if r.contains("findings") || r.contains("\\\"") => json_unescape(&r),
        _ => stdout.to_string(),
    };
    let verdict = extract_json_str(&text, "result").map(|v| json_unescape(&v));
    let findings = extract_json_str(&text, "findings")
        .map(|v| json_unescape(&v))
        .unwrap_or_default();
    match verdict.as_deref().map(|s| s.trim()) {
        Some("pass") => ("pass".to_string(), findings),
        Some("fail") => ("fail".to_string(), findings),
        _ => {
            let reason = if !findings.is_empty() {
                findings
            } else {
                format!("unparseable gate output: {}", text.chars().take(160).collect::<String>())
            };
            ("error".to_string(), reason)
        }
    }
}

/// Build the headless system prompt for a gate: the gate's SKILL.md plus a strict
/// output contract. Pure so the contract is unit-pinned.
pub fn gate_system_prompt(skill_md: &str) -> String {
    format!(
        "{}\n\n---\nYou are running HEADLESS as this gate. Review the card AC and the \
         branch diff provided in the user message against the gate's criteria above. \
         Output ONLY a single JSON object and nothing else: \
         {{\"result\":\"pass\",\"findings\":\"<one concise line>\"}} \
         where result is \"pass\" or \"fail\". No prose, no markdown, no code fence.",
        skill_md.trim()
    )
}

/// Run one gate headless and record its verdict. Best-effort: any failure path
/// records an "error" verdict (visible on the decision surface), never panics,
/// never silently passes.
/// #3468/#3471 — fold a pre-run chorus-health report into the OPS gate's context.
/// The ops gate judges health but can't fetch it (claude -p has `--disallowedTools
/// Bash`); the gate-runner pre-runs chorus-health (it can shell) and the gate reads
/// the report here instead of trying to execute one. ops-only; other gates pass
/// through unchanged. Pure + unit-pinned.
pub fn ops_ctx(base: &str, gate: &str, health: &str) -> String {
    if gate != "ops" {
        return base.to_string();
    }
    format!(
        "{}\n\n=== chorus-health --verbose (PRE-RUN by the gate-runner — JUDGE this output for pass/fail; do NOT run chorus-health yourself, you have no shell) ===\n{}",
        base, health
    )
}

/// #3625 AC2 — memory floor before every headless-claude spawn. Born from the
/// 2026-07-07 Library OOM: swap went 4.7→20GB in 15 minutes while gate claudes,
/// an Explore fanout, and werk env-up overlapped on the 16GB box; Jeff hard
/// powered off. Each gate/review below adds a 0.5–2GB process — spawning into
/// an already-spiraling box is what turns pressure into a seizure.
///
/// Parse `sysctl vm.swapusage` "used" into bytes. Handles M/G/K suffixes.
pub fn parse_swap_used_bytes(sysctl_out: &str) -> Option<u64> {
    let rest = sysctl_out.split("used =").nth(1)?;
    let tok = rest.split_whitespace().next()?;
    let (num, unit) = tok.split_at(tok.len().saturating_sub(1));
    let val: f64 = num.parse().ok()?;
    let mult = match unit {
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((val * mult) as u64)
}

/// Parse `memory_pressure -Q` "System-wide memory free percentage: NN%".
pub fn parse_free_pct(out: &str) -> Option<u8> {
    let rest = out.split("free percentage:").nth(1)?;
    rest.trim().trim_end_matches('%').split('%').next()?.trim().parse().ok()
}

/// true = proceed. A metric that can't be measured never blocks (hosted CI,
/// non-macOS); free memory under its floor always does.
///
/// #4497 — swap used no longer blocks. macOS keeps swap allocated long after a
/// spike, so on 2026-10-10 swap sat at 16.7 of 18.4 GB from 18:51 while 29% of
/// memory was free, and every demo gate waited 5 minutes on a number that had
/// stopped meaning anything. Free memory is the live signal; swap is still
/// sampled and recorded on the deferred event (and alerting owns "swap full").
pub fn memory_floor_verdict(
    swap_used: Option<u64>,
    free_pct: Option<u8>,
    swap_floor: u64,
    free_floor: u8,
) -> bool {
    let _ = (swap_used, swap_floor);
    if let Some(free) = free_pct {
        if free < free_floor {
            return false;
        }
    }
    true
}

/// Testable wait core: pull samples until one clears the floor or max_checks
/// is spent. Returns (cleared, checks_consumed). The caller owns the sleeping.
pub fn wait_for_floor_core(
    samples: impl Iterator<Item = (Option<u64>, Option<u8>)>,
    swap_floor: u64,
    free_floor: u8,
    max_checks: u32,
) -> (bool, u32) {
    let mut checks = 0;
    for (swap, free) in samples {
        checks += 1;
        if memory_floor_verdict(swap, free, swap_floor, free_floor) {
            return (true, checks);
        }
        if checks >= max_checks {
            return (false, checks);
        }
    }
    (false, checks)
}

fn sample_memory() -> (Option<u64>, Option<u8>) {
    let swap = Command::new("sysctl")
        .arg("vm.swapusage")
        .output()
        .ok()
        .and_then(|o| parse_swap_used_bytes(&String::from_utf8_lossy(&o.stdout)));
    let free = Command::new("memory_pressure")
        .arg("-Q")
        .output()
        .ok()
        .and_then(|o| parse_free_pct(&String::from_utf8_lossy(&o.stdout)));
    (swap, free)
}

/// Block until the box has headroom for one more headless claude, bounded.
/// Floors/wait are env-tunable (CHORUS_MEM_FREE_FLOOR_PCT, default 10;
/// CHORUS_MEM_WAIT_SECS, default 300). CHORUS_MEM_SWAP_FLOOR_GB is still read
/// and recorded but no longer blocks (#4497). CHORUS_MEM_FLOOR_DISABLE
/// skips entirely. Returns false when pressure never cleared — the caller
/// records a visible refusal, never spawns anyway.
fn wait_for_memory_floor(home: &Path, role: &str, card: u64, label: &str, round: &str) -> bool {
    if env::var("CHORUS_MEM_FLOOR_DISABLE").is_ok() {
        return true;
    }
    let swap_floor = env::var("CHORUS_MEM_SWAP_FLOOR_GB")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(8)
        * 1024 * 1024 * 1024;
    let free_floor: u8 = env::var("CHORUS_MEM_FREE_FLOOR_PCT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let wait_secs: u64 = env::var("CHORUS_MEM_WAIT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    const POLL_SECS: u64 = 15;
    let max_checks = (wait_secs / POLL_SECS).max(1) as u32;

    let mut deferred = false;
    let mut checks = 0u32;
    loop {
        let (swap, free) = sample_memory();
        checks += 1;
        if memory_floor_verdict(swap, free, swap_floor, free_floor) {
            if deferred {
                let trace = run_trace();
                jsonl(home, role, card, &trace, "demo.memory.cleared",
                      &format!(",\"label\":\"{}\",\"round\":\"{}\",\"checks\":{}", label, round, checks));
            }
            return true;
        }
        if !deferred {
            deferred = true;
            let trace = run_trace();
            jsonl(home, role, card, &trace, "demo.memory.deferred",
                  &format!(",\"label\":\"{}\",\"round\":\"{}\",\"swap_bytes\":{},\"free_pct\":{}",
                           label, round, swap.unwrap_or(0), free.unwrap_or(0)));
            emit_spine(home, "demo.memory.deferred", role, card, &trace);
        }
        if checks >= max_checks {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_secs(POLL_SECS));
    }
}

const GATE_OUTPUT_SCHEMA: &str = r#"{"type":"object","properties":{"result":{"type":"string","enum":["pass","fail","error"]},"findings":{"type":"string"}},"required":["result","findings"],"additionalProperties":false}"#;
const REVIEW_OUTPUT_SCHEMA: &str = r#"{"type":"object","properties":{"result":{"type":"string","enum":["pass","concerns","block"]},"findings":{"type":"string"}},"required":["result","findings"],"additionalProperties":false}"#;

fn agent_bin() -> String {
    env::var("CHORUS_AGENT_BIN").unwrap_or_else(|_| format!("{}/.chorus/bin/chorus-agent", env::var("HOME").unwrap_or_default()))
}

fn gate_profile() -> Option<String> {
    env::var("CHORUS_GATE_PROFILE").ok().filter(|s| !s.trim().is_empty())
}

/// Encode arbitrary prompt text as a JSON string (werk-demo is std-only).
fn json_quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// #4424 AC3 — the chorus-agent `run` job for one gate or review. The profile
/// must be a no-tools job profile; chorus-agent refuses any other.
#[allow(clippy::too_many_arguments)]
fn headless_job_request(profile: &str, input: &str, instructions: &str, schema: &str, model: Option<&str>, timeout: u64, trace: &str, revision: &str) -> String {
    format!("{{\"version\":1,\"profile\":{},\"input\":{},\"instructions\":{},\"output_schema\":{},\"model\":{},\"timeout_secs\":{},\"trace_id\":{},\"input_revision\":{}}}",
        json_quote(profile), json_quote(input), json_quote(instructions), schema,
        model.map(json_quote).unwrap_or_else(|| "null".into()), timeout, json_quote(trace), json_quote(revision))
}

/// Drain both pipes while writing input, bound the whole process group.
fn bounded_headless(mut command: Command, input: String, timeout: u64) -> R<String> {
    use std::os::unix::process::CommandExt;
    command.process_group(0).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|e| format!("chorus-agent spawn failed: {e}"))?;
    let pid = child.id() as i32;
    let mut stdin = child.stdin.take().ok_or("chorus-agent stdin missing")?;
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
    fn drain(mut pipe: impl Read) -> Vec<u8> {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut chunk) {
            if n == 0 { break; }
            let room = (8 * 1024 * 1024usize).saturating_sub(kept.len());
            kept.extend_from_slice(&chunk[..n.min(room)]);
        }
        kept
    }
    let stdout = child.stdout.take().ok_or("chorus-agent stdout missing")?;
    let stderr = child.stderr.take().ok_or("chorus-agent stderr missing")?;
    let output = std::thread::spawn(move || drain(stdout));
    let diagnostics = std::thread::spawn(move || drain(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < Duration::from_secs(timeout) => sleep(Duration::from_millis(25)),
            Ok(None) => break Err(format!("chorus-agent timed out after {timeout}s (bounded, fail-loud)")),
            Err(e) => break Err(format!("chorus-agent wait failed: {e}")),
        }
    };
    // Descendants must not keep inherited pipes open after the runner exits.
    unsafe { kill(-pid, 9); }
    let _ = child.wait();
    let _ = writer.join();
    let err = diagnostics.join().unwrap_or_default();
    let bytes = output.join().map_err(|_| "chorus-agent output reader failed")?;
    if !status?.success() {
        return Err(format!("chorus-agent failed: {}", String::from_utf8_lossy(&err).chars().take(160).collect::<String>()));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// #4424 AC3 — None when CHORUS_GATE_PROFILE is unset (the Claude path runs,
/// unchanged); otherwise the reply from `chorus-agent run --text`.
fn gate_via_agent(input: &str, instructions: &str, schema: &str, revision: &str) -> Option<R<String>> {
    let profile = gate_profile()?;
    let timeout = env::var("CHORUS_GATE_TIMEOUT_SECS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(180).clamp(1, 3600);
    let model = env::var("CHORUS_GATE_MODEL").ok();
    let trace = env::var("CHORUS_TRACE_ID").unwrap_or_default();
    let payload = headless_job_request(&profile, input, instructions, schema, model.as_deref(), timeout, &trace, revision);
    let mut command = Command::new(agent_bin());
    command.args(["run", "--text"]).env("CHORUS_HEADLESS", "1").env_remove("CLAUDECODE");
    // chorus-agent enforces the job timeout; this outer bound also reaps a wedged runner.
    Some(bounded_headless(command, payload, timeout + 5))
}

/// #4424 AC4 — a peer's reply in the gather vocabulary. pass → pass,
/// concerns → concerns, block or fail → block; anything else is an error.
pub fn parse_review_verdict(stdout: &str) -> (String, String) {
    let text = match extract_json_str(stdout, "result") {
        Some(r) if r.contains("findings") || r.contains("\\\"") => json_unescape(&r),
        _ => stdout.to_string(),
    };
    let raw = extract_json_str(&text, "result").map(|v| json_unescape(&v));
    let findings = extract_json_str(&text, "findings").map(|v| json_unescape(&v)).unwrap_or_default();
    let verdict = match raw.as_deref().map(str::trim) {
        Some("pass") => "pass",
        Some("concerns") => "concerns",
        Some("block") | Some("fail") => "block",
        _ => {
            let reason = if findings.is_empty() { format!("unparseable review output: {}", text.chars().take(160).collect::<String>()) } else { findings };
            return ("error".to_string(), reason);
        }
    };
    (verdict.to_string(), findings)
}

/// #4474 reopen — the card's branch diff (`origin/main...HEAD`) in its werk,
/// or why it could not be read: no werk at that path, or git refused. Never an
/// empty string standing in for "could not read" (Silas, 2026-10-11 15:10).
pub fn branch_diff(werk: &str) -> Result<String, String> {
    if !Path::new(werk).is_dir() {
        return Err(format!("no werk at {}", werk));
    }
    let o = Command::new("git").arg("-C").arg(werk).args(["diff", "origin/main...HEAD"]).output()
        .map_err(|e| format!("git could not run in {}: {}", werk, e))?;
    if !o.status.success() {
        return Err(format!("git diff failed in {}: {}", werk, String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

fn run_one_gate(home: &Path, role: &str, card: u64, gate: &str, round: &str) {
    if !wait_for_memory_floor(home, role, card, gate, round) {
        record_gate(home, role, card, gate, "error",
                    "memory-pressure: swap/free floor not met after bounded wait — gate spawn refused, not piled onto a spiraling box (#3625)", round);
        return;
    }
    let skill = home.join(format!("skills/gate-{}/SKILL.md", gate));
    let skill_md = match fs::read_to_string(&skill) {
        Ok(s) => s,
        Err(e) => {
            record_gate(home, role, card, gate, "error",
                        &format!("gate skill unreadable {}: {}", skill.display(), e), round);
            return;
        }
    };
    let sys = gate_system_prompt(&skill_md);

    let werk_base = env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
        format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
    });
    let werk = format!("{}/{}-{}", werk_base, role, card);
    let card_view = run(&script_path(home, "cards"), &["view", &card.to_string()]).unwrap_or_default();
    // #4474 reopen (Silas, 2026-10-11): a diff that could not be read refuses
    // the gate. With role=false every in-run gate since 10-10 reviewed the
    // empty diff of chorus-werk/false-<card> and could only pass.
    let diff = match branch_diff(&werk) {
        Ok(d) => d,
        Err(why) => {
            record_gate(home, role, card, gate, "error", &format!("no branch diff to review: {}", why), round);
            return;
        }
    };
    // Cap the diff so a huge change can't blow the context budget.
    let diff_capped: String = diff.chars().take(60_000).collect();
    let base_ctx = format!(
        "CARD #{}\n{}\n\n=== BRANCH DIFF (origin/main...HEAD) ===\n{}",
        card, card_view, diff_capped
    );
    // #3468/#3471 — for the OPS gate, pre-run chorus-health HERE (the Rust gate-runner
    // can shell; the gate's claude -p cannot) and fold its full output + exit status
    // into the context. Captured regardless of exit code — chorus-health exits 1 on
    // FAIL, which the gate must still SEE to judge. Non-ops gates: base_ctx unchanged.
    let ctx = if gate == "ops" {
        let hp = script_path(home, "chorus-health");
        let health = Command::new(&hp)
            .arg("--verbose")
            .output()
            .map(|o| {
                format!(
                    "exit_status: {}\n{}{}",
                    o.status.code().map(|c| c.to_string()).unwrap_or_else(|| "killed-by-signal".into()),
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr),
                )
            })
            .unwrap_or_else(|e| format!("(chorus-health failed to spawn: {})", e));
        ops_ctx(&base_ctx, gate, &health)
    } else {
        base_ctx
    };

    if let Some(job) = gate_via_agent(&ctx, &sys, GATE_OUTPUT_SCHEMA, round) {
        match job {
            Ok(out) => { let (result, findings) = parse_gate_verdict(&out); record_gate(home, role, card, gate, &result, &findings, round); }
            Err(e) => record_gate(home, role, card, gate, "error", &e, round),
        }
        return;
    }
    let model = env::var("CHORUS_GATE_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
    let bin = match claude_bin() {
        Some(b) => b,
        None => {
            record_gate(home, role, card, gate, "error", "claude binary not resolvable", round);
            return;
        }
    };
    let child = Command::new(&bin)
        // #3471 — mark the gate's claude -p as HEADLESS so chorus-hooks'
        // owes_response_block (#3218 RESPOND-FIRST) exempts it. A headless gate has
        // no peer to answer; without this the nudge-block deadlocks the gate's own
        // shell → the 180s timeout / denied chorus-health that errored this card's
        // product+ops gates. owes_response_block reads env::var("CHORUS_HEADLESS").is_ok()
        // (any value), alongside GITHUB_ACTIONS/ACT — same exemption family.
        .env("CHORUS_HEADLESS", "1")
        .args([
            "-p",
            "--model", &model,
            "--permission-mode", "dontAsk",
            "--no-session-persistence",
            "--output-format", "text",
            "--disallowedTools", "Bash,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task",
            "--system-prompt", &sys,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            record_gate(home, role, card, gate, "error", &format!("claude spawn failed: {}", e), round);
            return;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(ctx.as_bytes());
        // stdin drops here → closed, so claude sees EOF and proceeds.
    }
    // #3443 (Silas's BOUNDED contract) — a hung model must NOT block the gate
    // forever. Poll for exit up to CHORUS_GATE_TIMEOUT_SECS (default 180); on
    // expiry, kill the child and record error-on-expiry (fail-LOUD, never green).
    let timeout_secs: u64 = env::var("CHORUS_GATE_TIMEOUT_SECS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(180);
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                sleep(Duration::from_millis(200));
            }
            Err(e) => {
                record_gate(home, role, card, gate, "error",
                            &format!("claude wait failed: {}", e), round);
                return;
            }
        }
    };
    let status = match status {
        Some(s) => s,
        None => {
            record_gate(home, role, card, gate, "error",
                        &format!("gate timed out after {}s — claude killed (bounded, fail-loud)", timeout_secs),
                        round);
            return;
        }
    };
    let mut stdout = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut stdout);
    }
    let mut stderr = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut stderr);
    }
    if !status.success() {
        record_gate(home, role, card, gate, "error",
                    &format!("claude exit {:?}: {}", status.code(), stderr.chars().take(160).collect::<String>()),
                    round);
        return;
    }
    let (result, findings) = parse_gate_verdict(&stdout);
    record_gate(home, role, card, gate, &result, &findings, round);
}

/// #3539 — wrap a peer's CLAUDE.md as the system prompt for their SPAWNED review.
/// The peer reviews as themselves (their identity), answers the 4 gather questions,
/// and ends with a machine-parsable verdict line. Identity capped so a large
/// CLAUDE.md can't blow the context budget.
pub fn review_system_prompt(peer: &str, identity: &str) -> String {
    let id: String = identity.chars().take(40_000).collect();
    format!(
        "You are {peer}. Your role identity (CLAUDE.md) follows. Review the peer's card + branch diff (provided next) AS {peer} WOULD — substantive, not a rubber stamp. Answer the four gather questions: (1) how does this impact your products, (2) how does it impact you and your domain, (3) are they over-building or under-planning, (4) does this strengthen the system or just please the room. Then END your reply with exactly one JSON line and nothing after it: {{\"result\":\"pass\",\"findings\":\"<your substance>\"}} — result is pass | concerns | block.\n\n=== {peer} identity (CLAUDE.md) ===\n{id}",
        peer = peer, id = id
    )
}

/// #3539 — run ONE peer's review as a spawned headless `claude -p` loaded with
/// that peer's CLAUDE.md + the card + the branch diff + the 4 questions, recording
/// the verdict as demo.gather.replied. Mirrors run_one_gate exactly — peer review
/// becomes a deterministic in-process GATE, not an awaited live-session reply (the
/// step-5 fix: spawn-and-record replaces nudge-and-wait, which had no reliable writer).
fn run_one_review(home: &Path, role: &str, card: u64, peer: &str, round: &str) {
    // #3625 AC2 — same memory floor as gates: a review is one more headless claude.
    if !wait_for_memory_floor(home, role, card, peer, round) {
        record_gather_replied(home, role, card, peer, "error",
            "memory-pressure: swap/free floor not met after bounded wait — review spawn refused (#3625)");
        return;
    }
    let _ = round; // verdict round/patch are stamped by record_gather_replied (current_round/current_patch_id)
    let peer_md = peer_persona_path(home, peer);
    let identity = match fs::read_to_string(&peer_md) {
        Ok(s) => s,
        Err(e) => {
            record_gather_replied(home, role, card, peer, "error",
                &format!("peer persona unreadable {}: {}", peer_md.display(), e));
            return;
        }
    };
    let sys = review_system_prompt(peer, &identity);

    let werk_base = env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
        format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
    });
    let werk = format!("{}/{}-{}", werk_base, role, card);
    let card_view = run(&script_path(home, "cards"), &["view", &card.to_string()]).unwrap_or_default();
    // #4474 reopen — a review of a diff that could not be read is no review
    let diff = match branch_diff(&werk) {
        Ok(d) => d,
        Err(why) => {
            record_gather_replied(home, role, card, peer, "error", &format!("no branch diff to review: {}", why));
            return;
        }
    };
    let diff_capped: String = diff.chars().take(60_000).collect();
    let ctx = format!(
        "CARD #{}\n{}\n\n=== BRANCH DIFF (origin/main...HEAD) ===\n{}",
        card, card_view, diff_capped
    );

    // A job profile's output is schema-checked, so the reply is the JSON alone.
    let job_sys = format!("{}\n\nOutput ONLY the JSON object, nothing before or after it.", sys);
    if let Some(job) = gate_via_agent(&ctx, &job_sys, REVIEW_OUTPUT_SCHEMA, round) {
        match job {
            Ok(out) => { let (verdict, findings) = parse_review_verdict(&out); record_gather_replied(home, role, card, peer, &verdict, &findings); }
            Err(e) => record_gather_replied(home, role, card, peer, "error", &e),
        }
        return;
    }
    let model = env::var("CHORUS_GATE_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
    let bin = match claude_bin() {
        Some(b) => b,
        None => {
            record_gather_replied(home, role, card, peer, "error", "claude binary not resolvable");
            return;
        }
    };
    let child = Command::new(&bin)
        .env("CHORUS_HEADLESS", "1") // exempt the review's claude -p from RESPOND-FIRST (#3218/#3471)
        .args([
            "-p",
            "--model", &model,
            "--permission-mode", "dontAsk",
            "--no-session-persistence",
            "--output-format", "text",
            "--disallowedTools", "Bash,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task",
            "--system-prompt", &sys,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            record_gather_replied(home, role, card, peer, "error", &format!("claude spawn failed: {}", e));
            return;
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(ctx.as_bytes());
    }
    let timeout_secs: u64 = env::var("CHORUS_GATE_TIMEOUT_SECS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(180);
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                sleep(Duration::from_millis(200));
            }
            Err(e) => {
                record_gather_replied(home, role, card, peer, "error", &format!("claude wait failed: {}", e));
                return;
            }
        }
    };
    let status = match status {
        Some(s) => s,
        None => {
            record_gather_replied(home, role, card, peer, "error",
                &format!("review timed out after {}s — claude killed (bounded, fail-loud)", timeout_secs));
            return;
        }
    };
    let mut stdout = String::new();
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_string(&mut stdout);
    }
    if !status.success() {
        let mut stderr = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut stderr);
        }
        record_gather_replied(home, role, card, peer, "error",
            &format!("claude exit {:?}: {}", status.code(), stderr.chars().take(160).collect::<String>()));
        return;
    }
    // #4424 AC4 — the gather vocabulary (pass|concerns|block); a reply that
    // can't be read is an error, never a "concerns".
    let (verdict, findings) = parse_review_verdict(&stdout);
    record_gather_replied(home, role, card, peer, &verdict, &findings);
}



/// #3263 — record the pipeline's test outcome into the witness so the informed-go
/// check can require it (and Jeff sees it before the go). Called by werk.yml's test
/// step via `werk-demo test-result <card> pass|fail`. pass AND fail are recorded —
/// a red test is VISIBLE, not hidden; blocking-on-red is #3190's promotion, but the
/// result existing at all is what makes "I didn't/can't test" unreachable for a go.
fn record_test_result(home: &Path, role: &str, card: u64, result: &str, coverage: &str) {
    let trace = run_trace();
    // #4012 — stamp the CONTENT this verdict was measured against, so a later
    // round at the same patch can reuse it instead of re-running (and so a round
    // at DIFFERENT content cannot).
    let patch = current_patch_id(role, card);
    jsonl(home, role, card, &trace, "demo.test_result",
          &format!(",\"result\":\"{}\",\"patch_id\":\"{}\"{}", result, patch, coverage_field(coverage)));
    emit_spine(home, "demo.test_result", role, card, &trace);
}

/// #3263 — run the card's tests in its werk and return whether they passed. The
/// DEMO runs them so "did the tests run?" is a recorded fact, not a claim to argue.
/// Default mirrors the pipeline's hermetic gate (cargo lib+bins); overridable via
/// CHORUS_DEMO_TEST_CMD. Any non-zero exit → fail (visible, never hidden).
fn run_card_tests(werk: &str) -> bool {
    let cmd = env::var("CHORUS_DEMO_TEST_CMD")
        .unwrap_or_else(|_| "cargo test --lib --bins --quiet".to_string());
    let script = format!("cd '{}' && {}", werk, cmd);
    run("bash", &["-c", &script]).is_ok()
}

/// #4012 — three states, not two. The default command is `cargo test`, and a werk
/// with no Cargo.toml cannot produce a cargo verdict at all; reporting that as
/// "fail" is an ABSENT measurement dressed as a failing one, which is the 631-row
/// lie this card exists to end. UNMEASURED is the honest third state — the same
/// distinction #4009 established for the nightly, applied to the demo surface.
/// An explicit CHORUS_DEMO_TEST_CMD means the caller named something runnable, so
/// we run it and report what it measured.
fn card_test_verdict(werk: &str) -> &'static str {
    let overridden = env::var("CHORUS_DEMO_TEST_CMD").is_ok();
    if !overridden && !Path::new(werk).join("Cargo.toml").is_file() {
        return "unmeasured";
    }
    if run_card_tests(werk) { "pass" } else { "fail" }
}

/// #4502 — the demo's step subcommands: `<step> <card> <role> [arg]`.
pub const DEMO_STEPS: [&str; 5] = ["validate", "run-gate", "run-review", "session-smoke", "announce"];

/// #4474 — whose werk the demo is for. A step names the builder role the way
/// every werk verb does (`<step> <card> <role>`, #4502), so a dagu step needs no
/// DEPLOY_ROLE (Jeff 2026-10-09: DEPLOY_ROLE is identity; a run must not assert
/// one). The recording subcommands (gate, test-result, go, verdict) keep
/// DEPLOY_ROLE as their source, unchanged from v1.
pub fn demo_role(argv: &[String], deploy_role: &str) -> R<String> {
    let named = match argv.first().map(String::as_str) {
        Some(step) if DEMO_STEPS.contains(&step) => argv.get(2),
        Some(first) if first.parse::<u64>().is_ok() => argv.get(1),
        _ => None,
    };
    match named.map(|r| r.trim()).filter(|r| !r.is_empty()).or(Some(deploy_role.trim()).filter(|r| !r.is_empty())) {
        Some(r) => Ok(r.to_string()),
        None => Err("no role: pass `werk-demo <step> <card> <role>` or set DEPLOY_ROLE".to_string()),
    }
}

/// CLI shim: parse args/env only, then call the testable core (blueprint pattern).
/// The demo steps (#4502) — `validate`, `run-gate <gate>`, `run-review <n>`,
/// `announce`, each `<step> <card> <role>` — plus the recording subcommands:
/// `gate` (record a gate result, #3237), `test-result` (werk-test's verdict),
/// `go` (Jeff's go) and `verdict` (the land's witness check).
pub fn run_demo() -> R<DemoOutcome> {
    let argv: Vec<String> = env::args().skip(1).collect();
    let role = demo_role(&argv, &env::var("DEPLOY_ROLE").unwrap_or_default())?;
    let home = env::var("CHORUS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&env::var("HOME").unwrap_or_default()).join("CascadeProjects/chorus"));

    let mut args = argv.into_iter();
    let first = args.next().ok_or_else(usage)?;

    if DEMO_STEPS.contains(&first.as_str()) {
        let card: u64 = args.next().and_then(|s| s.parse().ok()).ok_or_else(usage)?;
        let _role_arg = args.next();
        let role = role.trim();
        return match first.as_str() {
            "validate" => validate_step(card, role, &home),
            "run-gate" => run_gate_step(card, role, &home, &args.next().ok_or_else(usage)?),
            "run-review" => {
                let n: usize = args.next().and_then(|s| s.parse().ok()).ok_or_else(usage)?;
                run_review_step(card, role, &home, n)
            }
            "session-smoke" => session_smoke_step(card, role, &home),
            _ => announce_step(card, role, &home),
        };
    }

    if first == "gate" {
        let card: u64 = args
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or("usage: werk-demo gate <card> <gate> <result>")?;
        let gate = args.next().ok_or("usage: werk-demo gate <card> <gate> <result>")?;
        if !REQUIRED_GATES.contains(&gate.as_str()) {
            return Err(format!("unknown gate '{}' — one of {:?}", gate, REQUIRED_GATES));
        }
        let result = args.next().unwrap_or_else(|| "pass".to_string());
        let findings = args.collect::<Vec<_>>().join(" ");
        let round = current_round(role.trim(), card);
        record_gate(&home, role.trim(), card, &gate, &result, &findings, &round);
        return Ok(DemoOutcome {
            message: format!(
                "gate {} recorded for #{}: {} — {}",
                gate, card, result,
                if findings.is_empty() { "(no findings)" } else { findings.as_str() }
            ),
            exit: 0,
        });
    }

    if first == "test-result" {
        let card: u64 = args
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or("usage: werk-demo test-result <card> <pass|fail>")?;
        let result = args.next().unwrap_or_else(|| "pass".to_string());
        // #3983 — werk-test passes the card's own-test coverage as `<state>:<n>`
        let coverage = args.next().unwrap_or_default();
        record_test_result(&home, role.trim(), card, &result, &coverage);
        return Ok(DemoOutcome {
            message: format!("test result recorded for #{}: {}", card, result),
            exit: 0,
        });
    }

    if first == "go" {
        // #3511 — JEFF's GO: the human accept (DEC-048), recorded AFTER the demoer
        // presents the finished, gated variant and Jeff has seen it. This is the
        // SECOND block condition (with the peer gathers) that releases the demo to
        // merge. Recorded at round + patch_id so it survives a content-preserving
        // rebase, exactly like the gather replies. NOT a peer signal — only Jeff's.
        let card: u64 = args
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or("usage: werk-demo go <card>")?;
        let trace = run_trace();
        let round = current_round(role.trim(), card);
        let patch = current_patch_id(&role, card);
        jsonl(&home, role.trim(), card, &trace, "demo.go",
              &format!(",\"round\":\"{}\",\"patch_id\":\"{}\"", round, patch));
        emit_spine(&home, "demo.go", role.trim(), card, &trace);
        return Ok(DemoOutcome {
            message: format!("Jeff's GO recorded for #{} (round {}) — the demo releases to merge.", card, round),
            exit: 0,
        });
    }

    if first == "verdict" {
        // #3519 — THE WITNESS-GATED LAND, the verb werk.yml's merge now keys on (replacing
        // the demo-exit-0 gate that can never fire). Reads the witness and returns exit 0 iff
        // verdict_proven — gates + both peer gathers + Jeff's go, all patch-keyed for THIS
        // round. demo() still present-and-exits (#3511); the go records (`werk-demo go`);
        // THIS reads the witness and decides the land. Exit 2 = presented-not-landed (a clean
        // green stop, not an error), mirroring demo()'s present-and-exit contract.
        let card: u64 = args
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or("usage: werk-demo verdict <card>")?;
        let witness = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
        let round = current_round(role.trim(), card);
        let patch = current_patch_id(&role, card);
        let proven = verdict_proven(&witness, card, role.trim(), &round, &patch);
        return Ok(DemoOutcome {
            message: if proven {
                format!("[verdict] #{} PROVEN (round {}) — gates + gathers + Jeff's go all on the witness for this patch → land.", card, round)
            } else {
                let mut missing: Vec<&str> = Vec::new();
                if !announce_ready_full(&witness, card, role.trim(), &round, &patch, false) { missing.push("gates/gathers"); }
                if !jeff_go_recorded(&witness, card, &round, &patch) { missing.push("jeff-go"); }
                format!("[verdict] #{} NOT proven (round {}) — witness missing: [{}]. Presented, not landed.", card, round, missing.join(", "))
            },
            exit: if proven { 0 } else { 2 },
        });
    }

    // #4502 — the all-in-one `werk-demo <card> <role>` is gone: the demo is
    // dagu steps now. Refuse by name rather than run part of a demo.
    Err(usage())
}

fn usage() -> String {
    format!(
        "usage: werk-demo <{}> <card> <role> [gate|n] — the demo runs as these steps (#4502): \
         validate · run-gate <product|code|quality|arch|ops> · run-review <1|2> · announce; \
         recording: werk-demo gate|test-result|go|verdict <card> ...",
        DEMO_STEPS.join("|")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_counts_counts_checked_and_total() {
        let v = "  - [x] one\n  - [ ] two\n  - [x] three\nnot an ac line";
        let (checked, total) = ac_counts(v);
        assert_eq!(checked, 2);
        assert_eq!(total, 3);
    }

    // === #3443 AC1 — gate self-run parsing ===

    #[test]
    fn extract_json_str_reads_simple_field() {
        let j = r#"{"result":"pass","findings":"looks good"}"#;
        assert_eq!(extract_json_str(j, "result").as_deref(), Some("pass"));
        assert_eq!(extract_json_str(j, "findings").as_deref(), Some("looks good"));
        assert_eq!(extract_json_str(j, "missing"), None);
    }

    #[test]
    fn extract_json_str_stops_at_unescaped_quote_keeps_escaped() {
        let j = r#"{"findings":"has a \"quote\" inside"}"#;
        assert_eq!(extract_json_str(j, "findings").as_deref(), Some(r#"has a \"quote\" inside"#));
    }

    #[test]
    fn json_unescape_handles_common_escapes() {
        assert_eq!(json_unescape(r#"a \"b\" c\nd\\e"#), "a \"b\" c\nd\\e");
    }

    #[test]
    fn parse_gate_verdict_raw_pass() {
        let out = r#"{"result":"pass","findings":"AC met, demo evidence present"}"#;
        let (r, f) = parse_gate_verdict(out);
        assert_eq!(r, "pass");
        assert_eq!(f, "AC met, demo evidence present");
    }

    #[test]
    fn parse_gate_verdict_raw_fail() {
        let out = r#"{"result":"fail","findings":"no tests for AC3"}"#;
        let (r, f) = parse_gate_verdict(out);
        assert_eq!(r, "fail");
        assert_eq!(f, "no tests for AC3");
    }

    #[test]
    fn parse_gate_verdict_unwraps_json_envelope() {
        // claude -p --output-format json: the gate JSON is nested+escaped in `result`.
        let out = r#"{"type":"result","subtype":"success","result":"{\"result\":\"pass\",\"findings\":\"clean\"}","is_error":false}"#;
        let (r, f) = parse_gate_verdict(out);
        assert_eq!(r, "pass");
        assert_eq!(f, "clean");
    }

    #[test]
    fn parse_gate_verdict_garbled_is_error_never_pass() {
        let (r, f) = parse_gate_verdict("I think this looks fine to me, shipping it");
        assert_eq!(r, "error");
        assert!(!f.is_empty());
    }

    #[test]
    fn parse_gate_verdict_missing_result_is_error() {
        let (r, _) = parse_gate_verdict(r#"{"findings":"only findings, no verdict"}"#);
        assert_eq!(r, "error");
    }

    // #3623 drive-by hermeticity fix: these two tests mutate the SAME process-wide
    // env var and cargo runs test threads in parallel — they raced (red under act's
    // timing, green locally). Serialize them.
    static CLAUDE_BIN_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn claude_bin_honors_explicit_override() {
        let _g = CLAUDE_BIN_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // #3443 live fix — CHORUS_CLAUDE_BIN names the exact binary, so a stripped
        // job PATH (act host-native missing ~/.local/bin) can still resolve claude.
        std::env::set_var("CHORUS_CLAUDE_BIN", "/bin/sh"); // any existing executable
        let got = claude_bin();
        std::env::remove_var("CHORUS_CLAUDE_BIN");
        assert_eq!(got.as_deref(), Some("/bin/sh"));
    }

    #[test]
    fn claude_bin_ignores_override_that_does_not_exist() {
        let _g = CLAUDE_BIN_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("CHORUS_CLAUDE_BIN", "/no/such/claude/binary");
        let got = claude_bin();
        std::env::remove_var("CHORUS_CLAUDE_BIN");
        // falls through to PATH / known locations — must NOT return the bogus path
        assert_ne!(got.as_deref(), Some("/no/such/claude/binary"));
    }

    #[test]
    fn gate_system_prompt_embeds_skill_and_output_contract() {
        let p = gate_system_prompt("# Gate Product\nCheck the AC.");
        assert!(p.contains("Gate Product"));
        assert!(p.contains("\"result\":\"pass\""));
        assert!(p.contains("ONLY a single JSON object"));
    }

    #[test]
    fn review_system_prompt_embeds_peer_identity_and_verdict_contract() {
        // #3539 — the spawned peer review loads the peer's CLAUDE.md as identity and
        // must end on the same {result, findings} contract parse_gate_verdict reads.
        let p = review_system_prompt("silas", "# Silas — Ops\nYou own observability.");
        assert!(p.contains("You are silas"));
        assert!(p.contains("Silas — Ops")); // identity embedded
        assert!(p.contains("\"result\":\"pass\"")); // machine-parsable verdict contract
        assert!(p.contains("over-building or under-planning")); // the 4 gather questions
    }

    #[test]
    fn ac_counts_zero_when_no_ac() {
        let (checked, total) = ac_counts("no acceptance criteria here");
        assert_eq!(total, 0);
        assert_eq!(checked, 0);
    }

    #[test]
    fn json_str_field_tolerates_pretty_print() {
        assert_eq!(json_str_field("{ \"status\" : \"WIP\" }", "status"), Some("WIP".to_string()));
        assert_eq!(json_str_field("{\"status\":\"Done\"}", "status"), Some("Done".to_string()));
    }


    // #3237 — gate-evidence enforcement. The demo verdict can't be recorded
    // unless all 5 gates left a demo.gate.result in the witness for the card.
    fn gate_line(card: u64, gate: &str) -> String {
        format!(
            r#"{{"ts":1,"event":"demo.gate.result","role":"wren","card_id":{},"trace_id":"t","gate":"{}","round":"r1","result":"pass"}}"#,
            card, gate
        )
    }

    #[test]
    fn gates_missing_empty_when_all_five_recorded() {
        let w = REQUIRED_GATES
            .iter()
            .map(|g| gate_line(3237, g))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(gates_missing(&w, 3237, "r1", "").is_empty(), "all 5 gates present → none missing");
    }

    #[test]
    fn gates_missing_lists_absent_gates() {
        let w = format!("{}\n{}", gate_line(3237, "product"), gate_line(3237, "code"));
        assert_eq!(gates_missing(&w, 3237, "r1", ""), vec!["quality", "arch", "ops"]);
    }

    #[test]
    fn gates_missing_all_when_witness_empty() {
        assert_eq!(gates_missing("", 3237, "r1", "").len(), 5);
    }

    // === #3461 — gate/gather evidence survives a content-preserving rebase ===

    #[test]
    fn gathers_reply_survives_round_churn_via_patch_id() {
        // Both peers replied at round sha "rA" carrying patch-id "pidX". A rebase
        // churns the round to "rB" but the tree (patch-id) is unchanged.
        let w = "{\"event\":\"demo.gather.replied\",\"card_id\":3461,\"peer\":\"silas\",\"round\":\"rA\",\"patch_id\":\"pidX\"}\n\
                 {\"event\":\"demo.gather.replied\",\"card_id\":3461,\"peer\":\"kade\",\"round\":\"rA\",\"patch_id\":\"pidX\"}\n\
                 {\"event\":\"demo.gather.replied\",\"card_id\":3461,\"peer\":\"abby-normal\",\"round\":\"rA\",\"patch_id\":\"pidX\"}";
        // churned round rB, SAME patch pidX → replies still count (no re-nudge loop)
        assert!(gathers_missing(w, 3461, "wren", "rB", "pidX").is_empty(),
            "patch-id match must survive the round churn");
        // different patch (real content change) AND churned round → genuinely missing
        assert_eq!(gathers_missing(w, 3461, "wren", "rB", "pidY"), vec!["silas", "kade", "abby-normal"]);
        // empty patch → round-only fallback (no false match; #3459 parity)
        assert_eq!(gathers_missing(w, 3461, "wren", "rB", ""), vec!["silas", "kade", "abby-normal"]);
        // exact round still works
        assert!(gathers_missing(w, 3461, "wren", "rA", "").is_empty());
    }

    #[test]
    fn gates_survive_round_churn_via_patch_id() {
        let mut w = String::new();
        for g in ["product", "code", "quality", "arch", "ops"] {
            w.push_str(&format!("{{\"event\":\"demo.gate.result\",\"card_id\":3461,\"gate\":\"{}\",\"round\":\"rA\",\"patch_id\":\"pidX\"}}\n", g));
        }
        assert!(gates_missing(&w, 3461, "rB", "pidX").is_empty(), "gates survive rebase via patch-id");
        assert_eq!(gates_missing(&w, 3461, "rB", "pidY").len(), 5, "real content change → gates re-run");
        assert_eq!(gates_missing(&w, 3461, "rB", "").len(), 5, "empty patch → round-only");
    }

    #[test]
    fn line_keyed_round_or_patch() {
        let l = "{\"round\":\"rA\",\"patch_id\":\"pidX\"}";
        assert!(line_keyed(l, "rA", ""));            // round match
        assert!(line_keyed(l, "rB", "pidX"));        // patch match across churn
        assert!(!line_keyed(l, "rB", "pidY"));       // neither
        assert!(!line_keyed(l, "rB", ""));           // empty patch, wrong round → no match
    }




    // #3324 AUDIT — announce_skip_drives_tail_for_test_suite deleted: with skip=true
    // announce_ready is `true || …` — the assert could not fail (passes-by-definition).

    // #3352 — the full invariant: gates AND gathers-replied before any announce.
    #[test]
    fn team_review_shows_this_rounds_peers_only_4474() {
        let now = r#"{"ts":2,"event":"demo.gather.replied","role":"kade","card_id":4474,"trace_id":"t","peer":"wren","round":"r2","note":"ok"}"#;
        let old_self = r#"{"ts":1,"event":"demo.gather.replied","role":"false","card_id":4474,"trace_id":"t","peer":"kade","round":"r2","verdict":"block","note":"Branch diff is empty"}"#;
        let old_peer = r#"{"ts":1,"event":"demo.gather.replied","role":"kade","card_id":4474,"trace_id":"t","peer":"silas","round":"r1","verdict":"block","note":"stale"}"#;
        let w = format!("{}\n{}\n{}", old_self, old_peer, now);
        let out = render_role_feedback(&w, 4474, "kade", "r2", "");
        assert!(out.contains("wren: pass — ok"), "{out}");
        // NEGATIVE PROOF — the demoer's own old reply and another round's reply must not show
        assert!(!out.contains("kade:"), "the demoer reviewed itself: {out}");
        assert!(!out.contains("stale"), "an earlier round's review shown as this round's: {out}");
    }

    fn gather_line(card: u64, peer: &str) -> String {
        format!(
            r#"{{"ts":1,"event":"demo.gather.replied","role":"wren","card_id":{},"trace_id":"t","peer":"{}","round":"r1","note":"ack"}}"#,
            card, peer
        )
    }

    #[test]
    fn gather_peers_excludes_the_demoer() {
        assert_eq!(gather_peers("wren"), vec!["silas", "kade", "abby-normal"]);
        assert_eq!(gather_peers("kade"), vec!["wren", "silas", "abby-normal"]);
        // #4502 — every builder gets the same count, Abby included
        assert_eq!(gather_peers("abby-normal"), vec!["wren", "silas", "kade"]);
    }

    #[test]
    fn announce_full_blocked_when_gates_present_but_gathers_missing() {
        // The 2026-06-11 class: all 5 gates recorded, NO peer feedback — the old
        // announce fired here and Jeff's go preceded the team's input. Now: standby.
        let w = REQUIRED_GATES.iter().map(|g| gate_line(3352, g)).collect::<Vec<_>>().join("\n");
        assert!(!announce_ready_full(&w, 3352, "wren", "r1", "", false), "gates without gathers ⇒ no announce");
        assert_eq!(gathers_missing(&w, 3352, "wren", "r1", ""), vec!["silas", "kade", "abby-normal"]);
    }

    #[test]
    fn announce_full_blocked_when_only_one_peer_replied() {
        let mut lines: Vec<String> = REQUIRED_GATES.iter().map(|g| gate_line(3352, g)).collect();
        lines.push(gather_line(3352, "kade"));
        let w = lines.join("\n");
        assert!(!announce_ready_full(&w, 3352, "wren", "r1", "", false), "one peer's reply is not the team's feedback");
        assert_eq!(gathers_missing(&w, 3352, "wren", "r1", ""), vec!["silas", "abby-normal"]);
    }

    #[test]
    fn announce_full_fires_when_gates_and_both_gathers_replied() {
        let mut lines: Vec<String> = REQUIRED_GATES.iter().map(|g| gate_line(3352, g)).collect();
        lines.push(gather_line(3352, "silas"));
        lines.push(gather_line(3352, "kade"));
        lines.push(gather_line(3352, "abby-normal"));
        let w = lines.join("\n");
        assert!(announce_ready_full(&w, 3352, "wren", "r1", "", false), "gates + every peer's reply ⇒ announce");
    }



    #[test]
    fn gather_sent_alone_does_not_satisfy_replied() {
        // Sending the 4 questions is not receiving feedback — only replied counts.
        let mut lines: Vec<String> = REQUIRED_GATES.iter().map(|g| gate_line(3352, g)).collect();
        lines.push(format!(
            r#"{{"ts":1,"event":"demo.gather.sent","role":"wren","card_id":3352,"trace_id":"t","peer":"silas","round":"r1","note":""}}"#
        ));
        lines.push(gather_line(3352, "kade"));
        let w = lines.join("\n");
        assert!(!announce_ready_full(&w, 3352, "wren", "r1", "", false), "sent != replied");
    }

    // #3365 — THE core proof: a prior round's records never satisfy this round.
    #[test]
    fn round2_with_only_round1_records_refuses_everything() {
        let mut lines: Vec<String> = REQUIRED_GATES.iter().map(|g| gate_line(3365, g)).collect();
        lines.push(gather_line(3365, "silas"));
        lines.push(gather_line(3365, "kade"));
        lines.push(gather_line(3365, "abby-normal"));
        let w = lines.join("\n"); // a fully-passed ROUND r1
        assert!(announce_ready_full(&w, 3365, "wren", "r1", "", false), "r1 evidence satisfies r1");
        assert!(!announce_ready_full(&w, 3365, "wren", "r2", "", false), "r1 evidence must NOT satisfy r2");
        assert_eq!(gates_missing(&w, 3365, "r2", "").len(), 5, "all gates fresh per round");
        assert_eq!(gathers_missing(&w, 3365, "wren", "r2", "").len(), 3, "all feedback fresh per round");
    }

    #[test]
    fn pre_3365_records_without_round_field_match_nothing() {
        // Old evidence (no round field) ages out structurally on landing.
        let w = r#"{"ts":1,"event":"demo.gate.result","role":"wren","card_id":3365,"trace_id":"t","gate":"product","result":"pass"}"#;
        assert_eq!(gates_missing(w, 3365, "r1", "").len(), 5);
    }

    // #3365 cross-verb contract (Kade's ACK ask): werk-demo's current_round and
    // werk-merge's head_sha[..12] MUST resolve identically for the same repo
    // state. Pinned here against a real git repo: short=12 == full-sha[..12]
    // (git guarantees prefix identity); Scenario G in werk-merge pins the other
    // side by seeding the announce with head_sha[..12] and merging against it.
    #[test]
    fn current_round_matches_merge_side_derivation() {
        let dir = std::env::temp_dir().join(format!("wd-round-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("wren-9999")).unwrap();
        let werk = dir.join("wren-9999");
        let git = |args: &[&str]| {
            std::process::Command::new("git").arg("-C").arg(&werk).args(args)
                .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
                .output().unwrap()
        };
        git(&["init", "-q", "."]);
        std::fs::write(werk.join("f"), "x").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "c"]);
        let full = String::from_utf8(git(&["rev-parse", "HEAD"]).stdout).unwrap().trim().to_string();
        std::env::remove_var("CHORUS_DEMO_ROUND");
        std::env::set_var("CHORUS_WERK_BASE", dir.to_str().unwrap());
        let demo_side = current_round("wren", 9999);
        std::env::remove_var("CHORUS_WERK_BASE");
        assert_eq!(demo_side, full[..12].to_string(), "demo-side round == merge-side head_sha[..12]");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_round_env_override_wins() {
        std::env::set_var("CHORUS_DEMO_ROUND", "test-round-x");
        assert_eq!(current_round("wren", 3365), "test-round-x");
        std::env::remove_var("CHORUS_DEMO_ROUND");
    }

    // #3305 — the gather re-fire + cross-role round-unknown family.

    #[test]
    fn a_demo_nudge_goes_through_nudge_send_from_and_to() {
        // #4515 — no old MCP server: the Rust nudge path, sender then recipient
        let a = nudge_send_args("wren", "silas", "msg \"quoted\"");
        assert_eq!(&a[..4], &["nudge-send", "wren", "silas", "msg \"quoted\""]);
    }

    #[test]
    fn the_announce_is_a_notice_never_a_reply_owed() {
        // #4502 — the announce asks Jeff, it does not make the builder owe a
        // reply (a reply is one more message). Negative proof: no "reply".
        let a = nudge_send_args("system", "wren", "msg");
        assert_eq!(&a[4..], &["--expects", "none"], "{:?}", a);
        assert!(!a.contains(&"reply".to_string()), "{:?}", a);
    }






    #[test]
    fn round_for_card_resolves_owners_werk_regardless_of_invoker() {
        // #3305 face two: silas records a gather on KADE's card — the round must
        // resolve from the CARD's werk (kade-<card>), not the invoker's role path
        // (silas-<card>, which doesn't exist → "round-unknown" → record invisible
        // to the announce gate; live specimen 2026-06-12 13:11 witness line).
        let base = std::env::temp_dir().join(format!("w3305-{}", std::process::id()));
        let werk = base.join("kade-9305");
        std::fs::create_dir_all(&werk).unwrap();
        for args in [vec!["init", "-q"], vec!["commit", "-q", "--allow-empty", "-m", "x"]] {
            let mut c = std::process::Command::new("git");
            c.arg("-C").arg(&werk);
            if args[0] == "commit" {
                c.env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
                 .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t");
            }
            assert!(c.args(&args).status().unwrap().success());
        }
        let r = round_for_card(base.to_str().unwrap(), 9305);
        assert_ne!(r, "round-unknown", "card werk exists under ANOTHER role → must still resolve");
        assert_eq!(r.len(), 12, "short-12 sha");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn round_for_card_unknown_when_no_werk_exists() {
        let base = std::env::temp_dir().join(format!("w3305-none-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        assert_eq!(round_for_card(base.to_str().unwrap(), 9306), "round-unknown");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn gathers_missing_ignores_other_cards_comma_terminated() {
        let w = format!("{}\n{}", gather_line(33520, "silas"), gather_line(33520, "kade"));
        assert_eq!(gathers_missing(&w, 3352, "wren", "r1", "").len(), 3, "card 33520's gathers must not satisfy 3352");
    }

    #[test]
    fn gates_missing_ignores_other_cards_comma_terminated() {
        // a full gate set for card 31 must NOT satisfy card 3 (anti #3/#31 collision).
        let w = REQUIRED_GATES
            .iter()
            .map(|g| gate_line(31, g))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(gates_missing(&w, 3, "r1", "").len(), 5, "card 31's gates must not satisfy card 3");
    }

    // #3511 — JEFF's go matcher: round+patch-keyed, card-scoped, his go only.
    #[test]
    fn jeff_go_recorded_keys_on_round_card_and_patch() {
        let w = r#"{"event":"demo.go","role":"wren","card_id":3511,"trace_id":"t","round":"rA","patch_id":"pX"}"#;
        assert!(jeff_go_recorded(w, 3511, "rA", ""), "matches by round");
        assert!(jeff_go_recorded(w, 3511, "rOTHER", "pX"), "patch-tolerant: matches by patch across a rebase");
        assert!(!jeff_go_recorded(w, 3511, "rB", "pZ"), "no round and no patch match → not recorded");
        assert!(!jeff_go_recorded(w, 9999, "rA", ""), "card-scoped — another card's go doesn't count");
        assert!(!jeff_go_recorded("", 3511, "rA", ""), "empty witness → no go");
        // a peer gather reply is NOT a Jeff go (peer-blessing is never the verdict).
        let g = r#"{"event":"demo.gather.replied","card_id":3511,"peer":"kade","round":"rA"}"#;
        assert!(!jeff_go_recorded(g, 3511, "rA", ""), "a peer gather is not Jeff's go");
    }

    // #3284 — gate VERDICTS in the decision surface (AC7): show what each gate found.
    fn gate_line_result(card: u64, gate: &str, result: &str) -> String {
        format!(
            r#"{{"ts":1,"event":"demo.gate.result","role":"wren","card_id":{},"trace_id":"t","gate":"{}","round":"r1","result":"{}"}}"#,
            card, gate, result
        )
    }

    #[test]
    fn gate_verdicts_reads_each_recorded_result() {
        let w = format!(
            "{}\n{}",
            gate_line_result(3284, "product", "pass"),
            gate_line_result(3284, "code", "fail")
        );
        let map: std::collections::HashMap<_, _> = gate_verdicts(&w, 3284).into_iter().collect();
        assert_eq!(map["product"], "pass");
        assert_eq!(map["code"], "fail");
        assert_eq!(map["quality"], "-", "a not-run gate is '-', never a fake pass");
    }

    #[test]
    fn gate_verdicts_last_result_wins_on_rerun() {
        let w = format!(
            "{}\n{}",
            gate_line_result(3284, "arch", "fail"),
            gate_line_result(3284, "arch", "pass")
        );
        let map: std::collections::HashMap<_, _> = gate_verdicts(&w, 3284).into_iter().collect();
        assert_eq!(map["arch"], "pass", "the latest result for a re-run gate wins");
    }

    // #3284 (AC1-4) — execution-state cockpit.
    #[test]
    fn execution_state_marks_completed_verbs_from_real_events() {
        let events = r#"{"events":[
          {"event":"commit.completed","card_id":3284},
          {"event":"push.completed","card_id":3284},
          {"event":"build.completed","card_id":3284}
        ]}"#;
        let s = render_execution_state(events, Some("pass"));
        assert!(s.contains("commit ✓"), "{}", s);
        assert!(s.contains("push ✓"), "{}", s);
        assert!(s.contains("build ✓"), "{}", s);
        assert!(s.contains("test ✓"), "test pass → ✓: {}", s);
        assert!(s.contains("deploy-werk -"), "no deploy event → -: {}", s);
        assert!(s.contains("env-up -"), "no env-up event → -: {}", s);
        assert!(s.contains("▸ demo ◂"), "demo is the HERE marker: {}", s);
        assert!(s.contains("on GO → merge · sync · deploy · accept"), "{}", s);
    }

    #[test]
    fn execution_state_test_fail_is_advisory_warn() {
        let s = render_execution_state(r#"{"events":[]}"#, Some("fail"));
        assert!(s.contains("test ⚠"), "fail test → ⚠ advisory, not a block: {}", s);
    }

    #[test]
    fn execution_state_empty_degrades_honestly_never_fake_checks() {
        // AC4: API unreachable → events_text "" → every verb is `-`, NEVER a ✓.
        let s = render_execution_state("", None);
        assert!(!s.contains('✓'), "no fabricated ✓ when there are no events: {}", s);
        assert!(s.contains("commit -") && s.contains("build -"), "all unknown: {}", s);
        assert!(s.contains("▸ demo ◂"), "still shows we're at demo: {}", s);
    }

    #[test]
    fn render_gate_summary_marks_pass_fail_and_notrun() {
        let w = format!(
            "{}\n{}",
            gate_line_result(3284, "product", "pass"),
            gate_line_result(3284, "code", "fail")
        );
        let s = render_gate_summary(&w, 3284);
        assert!(s.contains("product ✓"), "pass → ✓: {}", s);
        assert!(s.contains("code ✗"), "fail → ✗: {}", s);
        assert!(s.contains("quality -"), "not-run → -: {}", s);
        assert!(!s.contains("(none run"), "the misleading 'optional' label is gone: {}", s);
    }

    // #3263 — informed-go contract.
    fn test_result_line(card: u64, result: &str) -> String {
        format!(
            r#"{{"ts":1,"event":"demo.test_result","role":"wren","card_id":{},"trace_id":"t","result":"{}"}}"#,
            card, result
        )
    }
    #[test]
    fn test_result_recorded_returns_latest() {
        let w = format!("{}\n{}", test_result_line(3263, "fail"), test_result_line(3263, "pass"));
        assert_eq!(test_result_recorded(&w, 3263), Some("pass".to_string()));
    }

    // #3638 — the badge sources the SAME RUN's werk-test verdict, trace-scoped.
    fn test_result_line_traced(card: u64, result: &str, trace: &str) -> String {
        format!(
            r#"{{"ts":1,"event":"demo.test_result","role":"wren","card_id":{},"trace_id":"{}","result":"{}"}}"#,
            card, trace, result
        )
    }

    #[test]
    fn same_trace_verdict_found_pass_and_fail() {
        // PASS propagates — the false-red on a green blocking run is the #3638 defect
        let w = test_result_line_traced(3638, "pass", "run-A");
        assert_eq!(test_result_recorded_for_trace(&w, 3638, "run-A"), Some("pass".to_string()));
        // and FAIL propagates too — the fix must never hide a real red
        let w = test_result_line_traced(3638, "fail", "run-A");
        assert_eq!(test_result_recorded_for_trace(&w, 3638, "run-A"), Some("fail".to_string()));
    }

    #[test]
    fn stale_round_verdict_never_satisfies_the_current_trace() {
        // a record from an EARLIER round (different trace) must not mask an unrun suite
        let w = test_result_line_traced(3638, "pass", "run-OLD");
        assert_eq!(test_result_recorded_for_trace(&w, 3638, "run-NEW"), None);
        // and an empty current trace never matches anything
        assert_eq!(test_result_recorded_for_trace(&w, 3638, ""), None);
    }

    #[test]
    fn same_trace_wrong_card_never_matches() {
        let w = test_result_line_traced(9999, "pass", "run-A");
        assert_eq!(test_result_recorded_for_trace(&w, 3638, "run-A"), None);
    }

    #[test]
    fn test_result_recorded_none_when_absent() {
        assert_eq!(test_result_recorded("", 3263), None);
    }

    #[test]
    fn test_result_recorded_ignores_other_cards() {
        assert_eq!(test_result_recorded(&test_result_line(31, "pass"), 3), None);
    }

}

#[cfg(test)]
mod test_badge_4012 {
    use super::*;
    use std::fs;
    // === #4012 — the test badge must report the state it measured ===
    //
    // Measured 2026-08-25 over the whole witness: 1375 demo.test_ran rows, 744
    // sourced from werk-test, 631 self-run — and every self-run recorded "fail".
    // 58 cards had a real pass overwritten by one. The cause is that the
    // suppression below was keyed on TRACE while #3955's fast path re-presents
    // identical content under a NEW trace, so the verdict already paid for that
    // patch was invisible and the generic cargo default ran in its place.
    //
    // Patch-keying is strictly stronger than trace-keying for what #3638 was
    // protecting (a stale record from DIFFERENT content must not suppress), and
    // the control below is what proves that rather than asserting it.

    fn tr_line(card: u64, trace: &str, patch: &str, result: &str) -> String {
        format!(
            r#"{{"ts":1,"event":"demo.test_result","role":"kade","card_id":{},"trace_id":"{}","patch_id":"{}","result":"{}"}}"#,
            card, trace, patch, result
        )
    }

    #[test]
    fn negative_proof_a_new_trace_at_the_same_patch_reuses_the_paid_verdict() {
        // The exact #4011 shape: round 29 recorded pass under trace t29; round 30
        // re-presents identical content under t30. Before #4012 this returned None
        // and the demo self-ran, recording a false fail over the pass.
        let w = tr_line(4011, "t29", "fe82611b", "pass");
        assert_eq!(
            test_result_recorded_for_patch(&w, 4011, "fe82611b").as_deref(),
            Some("pass"),
            "same patch under a new trace must reuse the verdict already paid for it"
        );
    }

    #[test]
    fn control_a_different_patch_does_not_reuse_it_so_the_states_stay_separable() {
        // Without this the fix could return the last result unconditionally and
        // still pass the proof above — which would reintroduce exactly the stale
        // masking #3638 was written to prevent.
        let w = tr_line(4011, "t29", "fe82611b", "pass");
        assert_eq!(
            test_result_recorded_for_patch(&w, 4011, "aaaa1111"), None,
            "a verdict recorded against DIFFERENT content must never suppress the self-run"
        );
    }

    #[test]
    fn a_verdict_for_another_card_at_the_same_patch_is_not_borrowed() {
        let w = tr_line(9999, "t29", "fe82611b", "pass");
        assert_eq!(test_result_recorded_for_patch(&w, 4011, "fe82611b"), None);
    }

    #[test]
    fn an_empty_patch_id_never_matches_rather_than_matching_everything() {
        // The dangerous degenerate case: if empty meant "any", every card would
        // reuse every verdict. Absence must refuse, not wildcard.
        let w = tr_line(4011, "t29", "", "pass");
        assert_eq!(test_result_recorded_for_patch(&w, 4011, ""), None);
    }

    #[test]
    fn the_latest_verdict_for_a_patch_wins_over_an_earlier_one() {
        let w = format!(
            "{}\n{}",
            tr_line(4011, "t1", "fe82611b", "fail"),
            tr_line(4011, "t2", "fe82611b", "pass")
        );
        assert_eq!(test_result_recorded_for_patch(&w, 4011, "fe82611b").as_deref(), Some("pass"));
    }

    // #4502 — both tests read CHORUS_DEMO_TEST_CMD and the control sets it; the
    // process env is shared across test threads, so they take turns (the
    // negative proof read "pass" once when the control's set landed mid-run).
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn negative_proof_an_unrunnable_suite_reads_unmeasured_not_fail() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // A werk with no Cargo.toml cannot produce a cargo verdict. Reporting that
        // as "fail" is the 631-row lie: an absent measurement dressed as a failing
        // one. UNMEASURED is the honest third state (#4009 established it for the
        // nightly; this is the same distinction on the demo surface).
        let dir = std::env::temp_dir().join("werk-demo-4012-nocargo");
        let _ = fs::create_dir_all(&dir);
        let _ = fs::remove_file(dir.join("Cargo.toml"));
        assert_eq!(card_test_verdict(dir.to_str().unwrap()), "unmeasured");
    }

    #[test]
    fn control_a_runnable_suite_still_reports_a_real_pass_or_fail() {
        let _env = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // The control for the proof above: with a runnable command the verdict is
        // measured, so "unmeasured" cannot be a constant the code always returns.
        let dir = std::env::temp_dir().join("werk-demo-4012-runnable");
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(dir.join("Cargo.toml"), "[package]\nname=\"x\"\n");
        std::env::set_var("CHORUS_DEMO_TEST_CMD", "true");
        let v = card_test_verdict(dir.to_str().unwrap());
        std::env::remove_var("CHORUS_DEMO_TEST_CMD");
        assert_eq!(v, "pass", "a suite that actually ran must report what it measured");
    }
}

// #4424 AC3/AC4 — gates and peer reviews on a non-Claude model go through
// `chorus-agent run --text` only when CHORUS_GATE_PROFILE names a profile; an
// unparseable peer reply is an error, never a "concerns".
#[cfg(test)]
mod gate_profile_tests_4424 {
    use super::*;
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A fake chorus-agent that records its argv and stdin, then answers.
    fn fake_agent(dir: &Path, reply: &str) -> PathBuf {
        let bin = dir.join("chorus-agent");
        fs::write(&bin, format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{0}/args'\ncat > '{0}/stdin'\nprintf '%s' '{1}'\n",
            dir.display(), reply)).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }
    fn scratch(tag: &str) -> PathBuf {
        let d = env::temp_dir().join(format!("werkdemo-4424-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn unset_gate_profile_never_calls_chorus_agent() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("unset");
        let bin = fake_agent(&dir, r#"{"result":"pass","findings":"x"}"#);
        env::remove_var("CHORUS_GATE_PROFILE");
        env::set_var("CHORUS_AGENT_BIN", &bin);
        assert!(gate_via_agent("input", "sys", GATE_OUTPUT_SCHEMA, "rev").is_none());
        assert!(!dir.join("args").exists(), "chorus-agent must not run without a profile");
        env::remove_var("CHORUS_AGENT_BIN");
    }

    #[test]
    fn set_gate_profile_runs_chorus_agent_with_a_json_job() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("set");
        let bin = fake_agent(&dir, r#"{"result":"fail","findings":"missing test"}"#);
        env::set_var("CHORUS_GATE_PROFILE", "gate-chat");
        env::set_var("CHORUS_AGENT_BIN", &bin);
        let out = gate_via_agent("card text \"quoted\"", "gate sys", GATE_OUTPUT_SCHEMA, "abc123").expect("profile set").unwrap();
        env::remove_var("CHORUS_GATE_PROFILE");
        env::remove_var("CHORUS_AGENT_BIN");
        assert_eq!(parse_gate_verdict(&out).0, "fail");
        assert_eq!(fs::read_to_string(dir.join("args")).unwrap().trim(), "run --text");
        let stdin = fs::read_to_string(dir.join("stdin")).unwrap();
        assert_eq!(extract_json_str(&stdin, "profile").as_deref(), Some("gate-chat"));
        assert_eq!(json_unescape(&extract_json_str(&stdin, "input").unwrap()), "card text \"quoted\"");
        assert_eq!(extract_json_str(&stdin, "input_revision").as_deref(), Some("abc123"));
        assert!(stdin.contains("\"output_schema\":{"));
    }

    #[test]
    fn set_gate_profile_with_missing_agent_is_not_available() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        env::set_var("CHORUS_GATE_PROFILE", "gate-chat");
        env::set_var("CHORUS_AGENT_BIN", "/nonexistent/chorus-agent");
        let available = headless_available();
        env::remove_var("CHORUS_GATE_PROFILE");
        env::remove_var("CHORUS_AGENT_BIN");
        assert!(!available);
    }

    #[test]
    fn unparseable_review_is_error_never_concerns() {
        assert_eq!(parse_review_verdict("no json here at all").0, "error");
        assert_eq!(parse_review_verdict(r#"{"result":"maybe","findings":"?"}"#).0, "error");
        // Negative proof: real verdicts keep their meaning.
        assert_eq!(parse_review_verdict(r#"{"result":"fail","findings":"bad"}"#).0, "block");
        assert_eq!(parse_review_verdict(r#"{"result":"block","findings":"bad"}"#).0, "block");
        assert_eq!(parse_review_verdict(r#"{"result":"concerns","findings":"hm"}"#), ("concerns".to_string(), "hm".to_string()));
        assert_eq!(parse_review_verdict(r#"{"result":"pass","findings":"ok"}"#).0, "pass");
    }
}

#[cfg(test)]
mod variant_port_4458 {
    use super::*;

    #[test]
    fn abby_normal_gets_the_fourth_slot() {
        assert_eq!(variant_port("abby-normal", 3343, 3340), 3346);
        assert_eq!(clearing_url_for("abby-normal"), "http://localhost:3484/");
        assert_eq!(variant_port("wren", 3343, 3340), 3345);
    }

    /// NEGATIVE PROOF: a role with no slot is not given someone else's port.
    #[test]
    fn negative_proof_an_unknown_role_gets_production() {
        assert_eq!(variant_port("abby", 3343, 3340), 3340);
        assert_eq!(clearing_url_for("ghost"), "http://localhost:3470/");
    }
}

#[cfg(test)]
mod own_tests_3983 {
    use super::{coverage_field, coverage_recorded, coverage_words};

    fn line(card: u64, extra: &str) -> String {
        format!("{{\"event\":\"demo.test_result\",\"card_id\":{},\"result\":\"pass\",\"patch_id\":\"p\"{}}}", card, extra)
    }

    #[test]
    fn the_badge_says_how_many_of_the_cards_own_tests_ran() {
        let w = line(3983, &coverage_field("covered:14"));
        assert_eq!(coverage_recorded(&w, 3983).as_deref(), Some("covered:14"));
        assert_eq!(coverage_words(Some("covered:14")), "own tests: 14 registered case(s) ran");
    }

    /// NEGATIVE PROOF: a card whose covering tests did not run reads
    /// ZERO-COVERAGE, and a run that recorded nothing reads "not recorded",
    /// never a count
    #[test]
    fn zero_coverage_and_nothing_recorded_are_visible_states() {
        let w = line(3983, &coverage_field("zero-coverage:0"));
        assert!(coverage_words(coverage_recorded(&w, 3983).as_deref()).contains("ZERO-COVERAGE"));
        assert_eq!(coverage_words(coverage_recorded(&line(3983, ""), 3983).as_deref()), "own tests: not recorded");
        assert_eq!(coverage_recorded(&line(39830, &coverage_field("covered:1")), 3983), None, "another card's line");
    }

    #[test]
    fn only_the_typed_states_are_written() {
        assert_eq!(coverage_field(""), "");
        assert_eq!(coverage_field("covered"), "");
        assert_eq!(coverage_field("made-up:3"), "");
        assert_eq!(coverage_field("no-code:0"), ",\"coverage\":\"no-code:0\"");
    }
}

/// #4625 — a Clearing v2 card's demo link opens that card's v2, never v1.
/// Jeff, 2026-10-18 16:37, on #4597: "seems no different/worse" — the link was v1.
#[cfg(test)]
mod clearing_v2_link_4625 {
    use super::*;

    fn paths(p: &[&str]) -> Vec<String> { p.iter().map(|s| s.to_string()).collect() }

    #[test]
    fn a_v2_card_links_to_its_v2_and_other_cards_keep_v1() {
        let v2 = paths(&["directing/clearing-v2/public/app.js"]);
        assert_eq!(clearing_url_for_card("abby-normal", &v2), "http://localhost:3504/");
        assert_eq!(clearing_url_for_card("silas", &v2), "http://localhost:3501/");
        assert_eq!(clearing_url_for_card("kade", &paths(&["platform/pulse/src/x.ts"])), clearing_url_for("kade"));
        assert_eq!(clearing_link_refusal(&v2, &clearing_url_for_card("wren", &v2)), None);
    }

    /// NEGATIVE PROOF: a v2 card announced with the v1 link is refused by name.
    #[test]
    fn a_v2_card_with_the_v1_link_is_refused() {
        let v2 = paths(&["directing/clearing-v2/src/server.ts"]);
        let why = clearing_link_refusal(&v2, &clearing_url_for("abby-normal")).expect("v1 link must be refused");
        assert!(why.contains("Clearing v2"), "{why}");
        assert_eq!(clearing_link_refusal(&paths(&["directing/clearing/src/x.ts"]), &clearing_url_for("kade")), None);
    }
}
