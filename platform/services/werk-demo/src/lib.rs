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

/// #4889 — each AC line of a card view, numbered from 1, with its check target as
/// "<kind> <target>" (`check: after <kind> <target>` defers; its target is the inner one),
/// or None when the AC names no check. Pure.
fn ac_targets(card_view: &str) -> Vec<(usize, Option<String>)> {
    card_view.lines().map(str::trim_start).filter(|t| t.starts_with("- [x]") || t.starts_with("- [ ]"))
        .enumerate().map(|(i, t)| {
            let target = t.rfind("check: ").map(|k| {
                let mut w = t[k + 7..].split_whitespace();
                let kind = match w.next().unwrap_or("") { "after" => w.next().unwrap_or(""), k => k };
                format!("{kind} {}", w.next().unwrap_or("")).trim().to_string()
            }).filter(|x| x.contains(' '));
            (i + 1, target)
        }).collect()
}

/// #4889 AC9 — what is wrong with a card's AC checks, one line each: an AC that names no
/// `check:`, and an AC whose check target another AC already uses (each AC's check tests
/// that AC alone). Empty when every AC names its own check. Pure.
pub fn ac_check_problems(card_view: &str) -> Vec<String> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    let mut problems = Vec::new();
    // #4889 (code review, run 17) — borg-prove needs one AC proven before the land; a card
    // whose every AC is `check: after` could never be Proven and its land would reopen it.
    // Refused here, before any review or go.
    let ac: Vec<&str> = card_view.lines().map(str::trim_start).filter(|t| t.starts_with("- [x]") || t.starts_with("- [ ]")).collect();
    if !ac.is_empty() && ac.iter().all(|t| t.contains("check: after ")) {
        problems.push("every AC is `check: after`: nothing proves this card before the land, so it can never be Proven".to_string());
    }
    for (n, target) in ac_targets(card_view) {
        match target {
            None => problems.push(format!("AC {n} names no check")),
            Some(tg) => match seen.iter().find(|(s, _)| *s == tg) {
                Some((_, m)) => problems.push(format!("AC {n} shares its check `{tg}` with AC {m}")),
                None => seen.push((tg, n)),
            },
        }
    }
    problems
}

/// #4889 AC8 — an AC whose `test` check is an unrelated test, one finding each: the file
/// is missing, or its header (`head_of`, the file's first lines) never names `#<card>`. A
/// test that proves an AC says which card it proves, so a check borrowed from elsewhere
/// is flagged until its header names this card. Findings are keyed `<file>:1 — item 1`,
/// /gate-code's item. Pure: `head_of` reads the file.
pub fn unrelated_checks(card_view: &str, card: u64, head_of: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let tag = format!("#{card}");
    let names_card = |h: &str| h.match_indices(&tag).any(|(i, _)| !h[i + tag.len()..].starts_with(|c: char| c.is_ascii_digit()));
    ac_targets(card_view).into_iter().filter_map(|(n, t)| {
        let path = t?.strip_prefix("test ")?.to_string();
        match head_of(&path) {
            None => Some(format!("{path}:1 — item 1 — AC {n}'s check does not exist")),
            Some(h) if !names_card(&h) => Some(format!("{path}:1 — item 1 — AC {n}'s check never names {tag}: an unrelated test")),
            Some(_) => None,
        }
    }).collect()
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
pub const REQUIRED_GATES: [&str; 3] = ["code", "arch", "quality"];


/// #3352 — the demo ceremony as an INVARIANT ordering, verb-enforced:
///   reviews recorded → announce → go (#4889: peer gathers retired).
/// Jeff's spec verbatim (2026-06-11): "slow / often skipped gates; role feedback
/// skipped or failed; announce happens b4 role feedback so go precedes that."
/// Behavioral discipline failed 4× across 2 roles (3282, 3334, 3269, 3343); the
/// only durable fix is structural. announce_ready_full is the single ready
/// predicate: every required review (code, arch, quality) recorded.
/// skip (act/headless/test-suite) skips BOTH identically — scoped at birth,
/// the #3318 lesson (never land-breaking-then-scope).
/// #3365 — ROUND identity: Jeff's five steps run per ROUND, not per card.
/// A round is the commit sha being demoed (the werk's HEAD). Every gate
/// and announce record carries it; every check matches on it — so a
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


pub fn announce_ready_full(witness: &str, card: u64, _role: &str, round: &str, patch_id: &str, skip: bool) -> bool {
    // #4889 — the reviews ARE the gates now (code, arch, quality): one list
    skip || gates_missing(witness, card, round, patch_id).is_empty()
}

/// #3519 — THE WITNESS-GATED LAND predicate. demo() present-and-exits (ALWAYS exit 2,
/// #3279/#3511) and so can never signal "proven" through its exit code; but werk.yml gated
/// the merge on demo exit-0 (#3499). The two contradict → the merge step never fired →
/// nothing landed. This is the predicate the merge keys on INSTEAD: the land is proven iff,
/// for THIS patch, the witness shows every required review
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
    format!("Demo brief for the owner — #{} · round {} · {} · {} · clearing {}", card, round, tally, url, clearing)
}

/// #4642 — the one place Jeff decides: his approvals list. The per-run sign link is retired; a
/// go is a row on the list (posted below), beside card asks.
pub fn approvals_link() -> String {
    let base = std::env::var("CHORUS_GO_ORIGIN").ok().filter(|s| !s.is_empty())
        .unwrap_or_else(|| "https://lightlifeurbangardens.com".to_string());
    format!("{base}/go/approvals")
}

/// #4875 — the automated announce, to the OWNER only: what ran, and the one next step. Green:
/// show the change working, then `werk-demo vouch`, which alone puts the go in front of Jeff.
/// Red: each red gate named, and no vouch offered. Never a go link. Pure.
pub fn owner_brief(card: u64, line: &str, gates_green: bool, reds: &[String]) -> String {
    if gates_green && reds.is_empty() {
        format!("{line} · for you, not Jeff: show it working, then `werk-demo vouch {card}` (with what is proven and what is not; your navigator vouches too)")
    } else {
        let named = if reds.is_empty() { "gates not green".to_string() } else { reds.join(", ") };
        format!("{line} · for you, not Jeff: red — {named} · fix and re-run; nothing goes to Jeff while a gate is red")
    }
}

/// #4875 — one vouch on the witness: the owner's (what was shown, proven, not proven) or the
/// navigator's (why they'd click go). Free text is flattened so the witness stays one JSON line
/// per event and `json_str_after` reads it back whole. Pure.
#[allow(clippy::too_many_arguments)]
pub fn vouch_line(card: u64, by: &str, as_: &str, round: &str, patch: &str,
                  shown: &str, proven: &str, not_proven: &str, why: &str) -> String {
    let f = |t: &str| t.replace(['"', '\\'], "'").replace(['\n', '\r'], " ");
    // Kade 09:37: a real time, never "ts":0
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    jsonl_line(ts, "demo.vouched", by, card, "vouch",
        &format!(",\"as\":\"{}\",\"round\":\"{}\",\"patch_id\":\"{}\",\"shown\":\"{}\",\"proven\":\"{}\",\"not_proven\":\"{}\",\"why\":\"{}\"",
                 f(as_), f(round), f(patch), f(shown), f(proven), f(not_proven), f(why)))
}

/// The latest vouch of a kind for this card and round (patch-keyed like the go), optionally
/// only from someone other than `not_by`. Pure.
fn vouch_of<'a>(witness: &'a str, card: u64, round: &str, patch: &str, as_: &str, not_by: Option<&str>) -> Option<&'a str> {
    let card_key = format!("\"card_id\":{},", card);
    let as_key = format!("\"as\":\"{as_}\"");
    witness.lines().rev().find(|l| l.contains("\"event\":\"demo.vouched\"") && l.contains(&card_key)
        && l.contains(&as_key) && line_keyed(l, round, patch)
        && not_by.is_none_or(|o| json_str_after(l, "role").is_some_and(|r| !r.is_empty() && r != o)))
}

/// #4875 AC8 — a link Jeff can open from his phone: http(s), and not a loopback host. Pure.
pub fn reachable_link(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    let rest = match u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")) { Some(r) => r, None => return false };
    // Kade c38c84e10: an IPv6 host is bracketed; split on ':' would leave "[" and let [::1] through.
    let host = match rest.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => rest.split(['/', ':', '?', '#']).next().unwrap_or(""),
    };
    !host.is_empty() && host != "localhost" && !host.starts_with("127.") && host != "0.0.0.0" && host != "::1" && host != "::"
}

/// #4875 AC2/AC3 — what still stands between this round and Jeff's go: any gate that did not
/// pass, an owner vouch with nothing shown, no navigator vouch from a role other than the owner.
/// Empty = the go may go to Jeff. Pure.
pub fn vouch_missing(witness: &str, card: u64, round: &str, patch: &str, owner: &str) -> Vec<String> {
    // Kade b1c438aeb (2): each gate's verdict for THIS round (patch-keyed), never an older round's pass.
    let card_key = format!("\"card_id\":{},", card);
    let mut missing: Vec<String> = REQUIRED_GATES.iter().filter_map(|g| {
        let gate_key = format!("\"gate\":\"{g}\"");
        let r = witness.lines().rev().find(|l| l.contains("\"event\":\"demo.gate.result\"")
                && l.contains(&card_key) && l.contains(&gate_key) && line_keyed(l, round, patch))
            .and_then(|l| json_str_after(l, "result")).unwrap_or_else(|| "-".to_string());
        (r != "pass").then(|| format!("{g} {r}"))
    }).collect();
    let own = vouch_of(witness, card, round, patch, "owner", None)
        .filter(|l| json_str_after(l, "role").as_deref() == Some(owner));
    let blank = |l: &str, k: &str| json_str_after(l, k).unwrap_or_default().trim().is_empty();
    match own {
        None => missing.push(format!("owner vouch ({owner})")),
        Some(l) if blank(l, "shown") => missing.push("owner vouch: nothing shown".into()),
        // #4875 AC8 (Jeff 08:53 "all links must be wifi not localhost"): Jeff opens it on his phone.
        Some(l) if !reachable_link(&json_str_after(l, "shown").unwrap_or_default()) =>
            missing.push("owner vouch: shown link is not reachable from Jeff's phone (no localhost/127.0.0.1; a wifi or public URL)".into()),
        // Kade b1c438aeb (3): a vouch that names no proof is not a vouch.
        Some(l) if blank(l, "proven") => missing.push("owner vouch: nothing proven".into()),
        Some(_) => {}
    }
    if vouch_of(witness, card, round, patch, "navigator", Some(owner)).is_none() {
        missing.push("navigator vouch (a role other than the owner)".into());
    }
    missing
}

/// #4875 (Kade b1c438aeb (1)) — the go already reached Jeff for this round: a later vouch must
/// not post a second row or send a second message. Pure.
pub fn go_already_asked(witness: &str, card: u64, round: &str, patch: &str) -> bool {
    let card_key = format!("\"card_id\":{},", card);
    witness.lines().any(|l| l.contains("\"event\":\"demo.go.asked\"") && l.contains(&card_key) && line_keyed(l, round, patch))
}

/// #4875 (Jeff 09:17 "yes add the ac"; 09:28 one parser) — the card's AC as a phone-sized
/// checklist, from `borg-prove ac <card>` (one tab line each: ✓|☐, statement, check kind), so the
/// go message and the prover read the AC through the same parser. Pure over that output.
pub fn ac_checklist(borg_ac: &str) -> String {
    borg_ac.lines().filter_map(|l| {
        let mut f = l.split('\t');
        let (mark, text) = (f.next()?, f.next()?.trim());
        if mark != "✓" && mark != "☐" { return None; }
        let short: String = text.chars().take(90).collect();
        Some(format!("{mark} {short}{}", if text.chars().count() > 90 { "…" } else { "" }))
    }).collect::<Vec<_>>().join("\n")
}


pub fn borg_prove_beside(exe_dir: Option<&Path>, fallback: String) -> String {
    exe_dir.map(|d| d.join("borg-prove")).filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().to_string()).unwrap_or(fallback)
}

/// #4875 (Kade 09:36) — the go row is posted with the owner's identity, so only the owner's own
/// vouch call posts it; a navigator who vouches last records and stops. Pure.
pub fn may_post_go(by: &str, owner: &str) -> bool { !owner.is_empty() && by == owner }

/// #4875 (Kade 09:40) — what the owner is told when the navigator's vouch completes the set. Pure.
pub fn owner_post_nudge(card: u64) -> String {
    format!("#{card}: both vouched and every gate passed. Run `werk-demo vouch {card} owner …` to post the go to Jeff.")
}

/// #4875 (Kade ba6102a8f; Jeff 09-09 "i NEVER want to be asked for go on a card with unfinished
/// ac") — the AC still unchecked on the card. Any one blocks the go. Pure.
pub fn ac_unfinished(card_view: &str) -> Vec<String> {
    let list = ac_checklist(card_view);
    // Kade 47ef7e508: an unreadable card (board down → "") must refuse, never pass as "nothing open".
    if list.is_empty() { return vec!["card AC unreadable".to_string()]; }
    list.lines().filter(|l| l.starts_with('☐'))
        .map(|l| format!("unfinished {}", l.trim_start_matches('☐').trim())).collect()
}

/// #4875 — the ONE message that reaches Jeff: what was shown, proven and not proven, both
/// "I'd click go", then the link. Sent only when `vouch_missing` is empty. Pure.
pub fn jeff_go_message(witness: &str, card: u64, round: &str, patch: &str, owner: &str, link: &str, ac: &str) -> String {
    let o = vouch_of(witness, card, round, patch, "owner", None).unwrap_or("");
    let n = vouch_of(witness, card, round, patch, "navigator", Some(owner)).unwrap_or("");
    let g = |l: &str, k: &str| json_str_after(l, k).unwrap_or_default();
    let not_proven = { let t = g(o, "not_proven"); if t.trim().is_empty() { "nothing named".to_string() } else { t } };
    let ac = if ac.trim().is_empty() { String::new() } else { format!("\n{ac}\n") };
    format!("#{card} ready for your go — shown: {} · proven: {} · not proven: {not_proven}{ac} · {owner}: I'd click go · {}: I'd click go ({}) · decide: {link}",
            g(o, "shown"), g(o, "proven"), g(n, "role"), g(n, "why"))
}



fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// #4642 — the go row the demo posts: kind go, its action exactly {card, run, round} with the
/// werk's full 40-hex HEAD (the land compares full HEADs). Pure.
pub fn go_approval_body(card: u64, run: &str, round_full: &str, title: &str) -> Result<String, String> {
    if run.is_empty() || !run.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(format!("no go row: the run id '{run}' is not an id"));
    }
    if round_full.len() != 40 || !round_full.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(format!("no go row: '{round_full}' is not the werk's full HEAD"));
    }
    let action = format!("{{\"card\":{card},\"run\":\"{run}\",\"round\":\"{round_full}\"}}");
    // #4771: the card title already starts "#<card> " — never "#4771 #4771 …"
    let title = title.strip_prefix(&format!("#{card} ")).unwrap_or(title);
    let subject = format!("#{card} {title}");
    Ok(format!("{{\"name\":\"go-{card}-{run}\",\"label\":{},\"approvalKind\":\"go\",\"approvalSubject\":{},\"approvalAction\":{}}}",
        json_str(&format!("go #{card}")), json_str(&subject), json_str(&action)))
}

/// #4771 — the full sha run `trace` pushed for `card`, from werk-push's record (the last
/// push.completed / push.idempotent line of that card and run). Pure.
pub fn pushed_sha(push_log: &str, card: u64, trace: &str) -> Option<String> {
    if trace.is_empty() { return None; }
    let card_key = format!("\"card_id\":{card},");
    let trace_key = format!("\"trace_id\":\"{trace}\"");
    push_log.lines().rev()
        .filter(|l| l.contains(&card_key) && l.contains(&trace_key))
        .filter(|l| l.contains("\"event\":\"push.completed\"") || l.contains("\"event\":\"push.idempotent\""))
        .find_map(|l| l.split("\"sha\":\"").nth(1).and_then(|r| r.split('"').next()).map(str::to_string))
        .filter(|s| s.len() == 40 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
}

/// The commit the go row binds: CHORUS_DEMO_HEAD (tests), else what this run pushed.
fn tested_head(home: &Path, card: u64, trace: &str) -> Option<String> {
    if let Ok(h) = env::var("CHORUS_DEMO_HEAD") {
        if !h.trim().is_empty() { return Some(h.trim().to_string()); }
    }
    let log = std::fs::read_to_string(home.join("ops/logs/werk-push.jsonl")).ok()?;
    pushed_sha(&log, card, trace)
}


/// Post the go row as the demoing role (the door stamps ownedBy from the verified token). The
/// token goes to curl on stdin, never argv. A refusal is loud: no row, no go to give.
fn post_go_approval(home: &Path, role: &str, body: &str) -> Result<(), String> {
    use std::io::Write as _;
    let mint = env::var("CHORUS_IDENTITY_TOKEN_BIN").unwrap_or_else(|_| script_path(home, "chorus-identity-token"));
    let t = Command::new(&mint).arg(role).output().map_err(|e| format!("identity mint: {e}"))?;
    let token = String::from_utf8_lossy(&t.stdout).trim().to_string();
    if !t.status.success() || token.is_empty() {
        return Err(format!("no identity for {role} to post the go row with"));
    }
    let door = env::var("ATHENA_MAKE_URL").unwrap_or_else(|_| "http://localhost:3360".into());
    let curl = env::var("WERK_DEMO_CURL").unwrap_or_else(|_| "/usr/bin/curl".into());
    let mut child = Command::new(&curl).args(["-sS", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "8", "-X", "POST",
            "-H", "Content-Type: application/json", "-H", "@-", "--data-binary", body, &format!("{door}/decisions/approvals")])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("door: {e}"))?;
    {
        let mut stdin = child.stdin.take().ok_or("door: no stdin")?;
        writeln!(stdin, "Authorization: Bearer {token}").map_err(|e| format!("door: {e}"))?;
    }
    let o = child.wait_with_output().map_err(|e| format!("door: {e}"))?;
    let code = String::from_utf8_lossy(&o.stdout).trim().to_string();
    // 409: this run's go row is already posted (a re-present of the same round) — fine.
    if code.starts_with('2') || code == "409" { Ok(()) } else { Err(format!("the door answered {code} to the go row")) }
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

/// #4875 — the verified caller: the OS account this process runs as (`chorus-<role>`), never an
/// argument or env a caller could set. None for any non-role account (Jeff never vouches).
fn verified_caller() -> Option<String> {
    let out = Command::new("id").arg("-un").output().ok()?;
    let user = String::from_utf8_lossy(&out.stdout).trim().to_string();
    user.strip_prefix("chorus-").filter(|r| !r.is_empty()).map(str::to_string)
}

fn announce_to_owner(from: &str, trace: &str, msg: &str) {
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



// --- the demo steps (#4502) ---
//
// #4502 (Jeff 2026-10-18 11:31, "that looks right") — the demo is dagu steps,
// not one binary run in a hidden order:
//   validate → [gate-code, -arch, -quality] in parallel → announce → Jeff's go.
// #4889 (Jeff 11-04 18:21): one code review, one architecture and quality review;
// the product/ops gates, peer reviews and session smoke are retired (their checks'
// new homes are in each gate skill's "Where the old checks went" table).
// Each step is one subcommand below; dagu shows which one stopped and retries
// it alone. Reviews inform; Jeff's go stays sovereign (#3263/DEC-048).

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
    // #4889 AC9 — every AC names its own check, before anything is reviewed
    let check_problems = ac_check_problems(&cv);
    if !check_problems.is_empty() {
        jsonl(home, role, card, &trace, "demo.refused", &fail_extra("ac-checks"));
        emit_spine_reason(home, "demo.refused", role, card, &trace, "ac-checks");
        return Err(format!("#{} AC checks: {}", card, check_problems.join("; ")));
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
/// unless it already passed this round (a retry after a pass is a no-op; a
/// retry after a fail runs the gate again, #4851). Exit 0 when a verdict is recorded; non-zero, named, when the gate
/// could not run (no headless model on the box, or an "error" result).
pub fn run_gate_step(card: u64, role: &str, home: &Path, gate: &str) -> R<DemoOutcome> {
    if !REQUIRED_GATES.contains(&gate) {
        return Err(format!("unknown gate '{}' — one of {:?}", gate, REQUIRED_GATES));
    }
    let round = current_round(role, card);
    let patch = current_patch_id(role, card);
    let what = format!("gate {}", gate);
    let recorded = |w: &str| step_result(w, card, "demo.gate.result", "gate", gate, "result", "findings", &round, &patch);
    // #4851 (13:52): only a pass is final for the round. A recorded fail is run
    // again on retry, so a red from a passing blip (the door slow for a second)
    // is not stuck until someone makes a commit just to get a new round.
    if let Some(done) = recorded(&read_witness(home)).filter(|(r, _)| r == "pass") {
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
    let mut out = step_outcome(&what, card, recorded(&read_witness(home)));
    // #4889 B7 (Wren 16:32: run 14's three red reviews reached the go step) — a red review
    // fails its step, so dagu runs nothing after it: the owner wakes, Jeff never sees it.
    // Under Jeff's 16:05 rule a round of notes only is a pass, and goes on.
    if out.exit == 0 && recorded(&read_witness(home)).is_some_and(|(r, _)| r == "fail") { out.exit = 1; }
    Ok(out)
}


/// #4502 — what announce still needs: the reviews (code, arch, quality, #4889) with
/// no usable result this round ("error" counts as missing — it never ran). Pure.
pub fn announce_missing(witness: &str, card: u64, _role: &str, round: &str, patch_id: &str) -> Vec<String> {
    let usable = |r: Option<(String, String)>| r.is_some_and(|(v, _)| v != "error");
    REQUIRED_GATES
        .iter()
        .filter(|g| !usable(step_result(witness, card, "demo.gate.result", "gate", g, "result", "findings", round, patch_id)))
        .map(|g| format!("gate-{}", g))
        .collect()
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
        let tally = format!("reviews {} · {} · AC {}/{}",
            if gates_green { "✓" } else { "✗" },
            test_summary.replace("tests: ", "tests "), checked, total);
        // #4875 — the announce goes to the owner, never Jeff, and posts no go row: the owner
        // shows the work, then vouches (`werk-demo vouch`); only the vouch puts the go in front
        // of Jeff. Log evidence: 11-03 06:58 #4873's go went out with "gates ✗ · AC 0/2".
        let line = announce_line(card, &round, &tally, &variant_url, &clearing_url);
        let reds: Vec<String> = gate_verdicts(&witness, card).into_iter()
            .filter(|(_, r)| r != "pass").map(|(g, r)| format!("{g} {r}")).collect();
        announce_to_owner(role, &trace, &owner_brief(card, &line, gates_green, &reds));
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
    record_gate_at(home, role, card, gate, result, findings, round, "");
}

/// #4889 (Jeff 11-05 10:06) — the same record, naming the commit the review read up to,
/// so the next round reviews only what changed since (`last_reviewed`).
#[allow(clippy::too_many_arguments)]
fn record_gate_at(home: &Path, role: &str, card: u64, gate: &str, result: &str, findings: &str, round: &str, commit: &str) {
    let trace = run_trace();
    // #3284 — feedback is REQUIRED: a gate carries WHAT it found, not just pass/fail,
    // so the announce shows real feedback. Sanitized for the one-line JSONL witness.
    let f = findings.replace('\\', " ").replace('"', "'").replace('\n', " ");
    let pid = current_patch_id(role, card); // #3461 — record patch-id so the result survives a rebase-churn
    jsonl(home, role, card, &trace, "demo.gate.result",
          &format!(",\"gate\":\"{}\",\"result\":\"{}\",\"round\":\"{}\",\"patch_id\":\"{}\",\"commit\":\"{}\",\"findings\":\"{}\"", gate, result, round, pid, commit, f));
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

/// #4889 (Kade 19:19: "derive the verdict from the findings, not the model's own word")
/// — a review's fail items, read from its SKILL.md frontmatter (`fail-items: 1-5, 7`).
/// The skill is the one home of its rule; the runner never hard-codes it. Pure.
pub fn fail_items(skill_md: &str) -> Vec<u32> {
    let line = skill_md.lines().take_while(|l| !l.starts_with("# ")).find_map(|l| l.strip_prefix("fail-items:"));
    let mut out = Vec::new();
    for part in line.unwrap_or("").split(',') {
        let part = part.trim();
        match part.split_once('-') {
            Some((a, b)) => if let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>()) { out.extend(a..=b) },
            None => if let Ok(n) = part.parse::<u32>() { out.push(n) },
        }
    }
    out
}

/// The JSON objects inside the first `"findings": [ ... ]` array of a review's reply
/// (raw, or nested escaped in an envelope's `result`). None when there is no such array.
fn findings_objects(stdout: &str) -> Option<Vec<String>> {
    let text = match extract_json_str(stdout, "result") {
        Some(r) if r.contains("findings") => json_unescape(&r),
        _ => stdout.to_string(),
    };
    array_objects(&text, "findings")
}

/// The JSON objects inside the first `"<key>": [ ... ]` array of `text`. None when
/// there is no such array, or it never closes.
fn array_objects(text: &str, key: &str) -> Option<Vec<String>> {
    let key = text.find(&format!("\"{key}\""))?;
    let open = key + text[key..].find('[')?;
    let (mut depth, mut in_str, mut esc, mut start) = (0i32, false, false, None);
    let mut objs = Vec::new();
    for (i, c) in text[open + 1..].char_indices() {
        let at = open + 1 + i;
        if in_str { if esc { esc = false } else if c == '\\' { esc = true } else if c == '"' { in_str = false } continue; }
        match c {
            '"' => in_str = true,
            '{' => { if depth == 0 { start = Some(at) } depth += 1 }
            '}' => { depth -= 1; if depth == 0 { if let Some(s0) = start.take() { objs.push(text[s0..=at].to_string()) } } }
            ']' if depth == 0 => return Some(objs),
            _ => {}
        }
    }
    None
}

/// #4889 — the verdict is derived from the findings: any finding on a fail item fails,
/// other findings are notes, an empty list passes. An empty `fail_on` (a skill that
/// names no fail-items) is strict: every finding fails, whatever its item number
/// (Kade 09:14: a 0..=99 range let item 100 pass). A reply with no findings list, or a
/// finding with no item number, is an error: never a pass. Pure + unit-tested.
pub fn gate_verdict(stdout: &str, fail_on: &[u32]) -> (String, String) {
    let Some(objs) = findings_objects(stdout) else {
        return ("error".to_string(), format!("unparseable review output: {}", stdout.chars().take(160).collect::<String>()));
    };
    let mut fails = Vec::new();
    let mut notes = Vec::new();
    for o in &objs {
        let item = o.find("\"item\"").and_then(|k| {
            let rest = o[k + 6..].trim_start().strip_prefix(':')?.trim_start();
            rest.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok()
        });
        let Some(item) = item else {
            return ("error".to_string(), format!("a finding names no item: {}", o.chars().take(160).collect::<String>()));
        };
        let at = extract_json_str(o, "where").map(|v| json_unescape(&v)).unwrap_or_default();
        let what = extract_json_str(o, "what").map(|v| json_unescape(&v)).unwrap_or_default();
        let line = format!("{at} — item {item} — {what}");
        if fail_on.is_empty() || fail_on.contains(&item) { fails.push(line) } else { notes.push(line) }
    }
    let result = if fails.is_empty() { "pass" } else { "fail" };
    let mut all = fails;
    all.extend(notes.into_iter().map(|n| format!("note: {n}")));
    (result.to_string(), if all.is_empty() { "no findings".to_string() } else { all.join(" | ") })
}

/// Build the headless system prompt for a review: its SKILL.md plus a strict output
/// contract. The model reports findings only; werk-demo derives the verdict. Pure.
pub fn gate_system_prompt(skill_md: &str) -> String {
    format!(
        "{}\n\n---\nYou are running HEADLESS as this review. Review the branch diff in the \
         user message against the numbered items above. Output ONLY a single JSON object \
         and nothing else: {{\"findings\":[{{\"item\":<number>,\"where\":\"file:line\",\"what\":\"<one line>\"}}]}} \
         with one entry per finding and an empty list when there are none. Do not give a \
         verdict: it is derived from your findings. No prose, no markdown, no code fence.",
        skill_md.trim()
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

const GATE_OUTPUT_SCHEMA: &str = r#"{"type":"object","properties":{"findings":{"type":"array","items":{"type":"object","properties":{"item":{"type":"integer"},"where":{"type":"string"},"what":{"type":"string"}},"required":["item","where","what"],"additionalProperties":false}}},"required":["findings"],"additionalProperties":false}"#;

fn agent_bin() -> String {
    env::var("CHORUS_AGENT_BIN").unwrap_or_else(|_| format!("{}/.chorus/bin/chorus-agent", env::var("HOME").unwrap_or_default()))
}

/// #4866 — CHORUS_GATE_PROFILE wins. Unset, an account with no claude (Abby,
/// on Gemini) uses the one profile in its own ~/.chorus/agent-profiles.json, so
/// its gates run as itself instead of failing "no headless model". An account
/// with claude keeps the Claude path; zero or several profiles choose nothing.
fn gate_profile() -> Option<String> {
    let set = env::var("CHORUS_GATE_PROFILE").ok();
    choose_gate_profile(set.as_deref(), claude_available, || {
        let home = env::var("HOME").ok()?;
        fs::read_to_string(Path::new(&home).join(".chorus/agent-profiles.json")).ok()
    })
}

/// #4866 — the rule, without the environment: an explicit profile wins; with
/// claude present, none; otherwise the account's only profile, or none.
pub fn choose_gate_profile(set: Option<&str>, has_claude: impl FnOnce() -> bool, profiles: impl FnOnce() -> Option<String>) -> Option<String> {
    if let Some(p) = set.filter(|s| !s.trim().is_empty()) {
        return Some(p.to_string());
    }
    if has_claude() {
        return None;
    }
    let names = profile_names(&profiles()?);
    if names.len() == 1 { names.into_iter().next() } else { None }
}

/// #4866 — the keys of the top-level "profiles" object (std-only JSON walk).
pub fn profile_names(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let (mut i, mut depth, mut names) = (0usize, 0i32, Vec::new());
    let mut start: Option<i32> = None; // depth of the profiles object once found
    let mut last_key_top = String::new();
    while i < b.len() {
        match b[i] {
            b'"' => {
                let s = i + 1;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' { i += 1; }
                    i += 1;
                }
                let word = String::from_utf8_lossy(&b[s..i.min(b.len())]).into_owned();
                let mut j = i + 1;
                while j < b.len() && (b[j] as char).is_whitespace() { j += 1; }
                let is_key = j < b.len() && b[j] == b':';
                if is_key && depth == 1 { last_key_top = word.clone(); }
                if is_key && start == Some(depth) { names.push(word); }
            }
            b'{' => {
                depth += 1;
                if start.is_none() && depth == 2 && last_key_top == "profiles" { start = Some(2); }
            }
            b'}' => {
                if start == Some(depth) { return names; }
                depth -= 1;
            }
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    names
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
        return Err(crashed_reviewer("chorus-agent", "failed", &String::from_utf8_lossy(&err), &String::from_utf8_lossy(&bytes)));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// #4424 AC3 — None when CHORUS_GATE_PROFILE is unset (the Claude path runs,
/// unchanged); otherwise the reply from `chorus-agent run --text`.
fn gate_via_agent(input: &str, instructions: &str, schema: &str, revision: &str) -> Option<R<String>> {
    let profile = gate_profile()?;
    let timeout = env::var("CHORUS_GATE_TIMEOUT_SECS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(600).clamp(1, 3600);
    let model = env::var("CHORUS_GATE_MODEL").ok();
    let trace = env::var("CHORUS_TRACE_ID").unwrap_or_default();
    let payload = headless_job_request(&profile, input, instructions, schema, model.as_deref(), timeout, &trace, revision);
    let mut command = Command::new(agent_bin());
    command.args(["run", "--text"]).env("CHORUS_HEADLESS", "1").env_remove("CLAUDECODE");
    // chorus-agent enforces the job timeout; this outer bound also reaps a wedged runner.
    Some(bounded_headless(command, payload, timeout + 5))
}


/// #4474 reopen — the card's branch diff (`origin/main...HEAD`) in its werk,
/// or why it could not be read: no werk at that path, or git refused. Never an
/// empty string standing in for "could not read" (Silas, 2026-10-11 15:10).
pub fn branch_diff(werk: &str) -> Result<String, String> {
    branch_diff_range(werk, "origin/main...HEAD")
}

/// #4889 — the diff of one range in the werk (`origin/main...HEAD`, or `<commit>..HEAD`
/// for a round that reads only what changed since the last one reviewed).
pub fn branch_diff_range(werk: &str, range: &str) -> Result<String, String> {
    if !Path::new(werk).is_dir() {
        return Err(format!("no werk at {}", werk));
    }
    let o = Command::new("git").arg("-C").arg(werk).args(["diff", range]).output()
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
    // #4889 bootstrap (Kade 20:14): the skill is read from canonical, never from the card
    // under review (a card must not rewrite its own reviewer). A skill with no fail-items
    // (main's, until this card lands) is read strictly: every finding fails.
    let fail_on = fail_items(&skill_md);
    let strict = fail_on.is_empty();

    let werk_base = env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
        format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
    });
    let werk = format!("{}/{}-{}", werk_base, role, card);
    let card_view = run(&script_path(home, "cards"), &["view", &card.to_string()]).unwrap_or_default();
    // #4474 reopen (Silas, 2026-10-11): a diff that could not be read refuses
    // the gate. With role=false every in-run gate since 10-10 reviewed the
    // empty diff of chorus-werk/false-<card> and could only pass.
    // #4889 (Jeff 11-05 10:06: "isnt there just a new commit to review?") — a round reads
    // only what changed since the commit this gate last reviewed; no earlier round, or a
    // rebase that left it behind, reads the whole branch (Kade 10:07).
    let head = run_in_dir(&werk, "git", &["rev-parse", "HEAD"]).map(|h| h.trim().to_string()).unwrap_or_default();
    let prior = last_reviewed(&fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default(), card, gate);
    let since = prior.as_ref().map(|(c, _)| c.clone())
        .filter(|c| !c.is_empty() && run_in_dir(&werk, "git", &["merge-base", "--is-ancestor", c, "HEAD"]).is_ok());
    let range = since.as_ref().map(|c| format!("{c}..HEAD")).unwrap_or_else(|| "origin/main...HEAD".to_string());
    let delta_files: Option<Vec<String>> = since.as_ref().map(|c| {
        run_in_dir(&werk, "git", &["diff", "--name-only", &format!("{c}..HEAD")]).unwrap_or_default()
            .lines().map(str::to_string).filter(|l| !l.is_empty()).collect()
    });
    let diff = match branch_diff_range(&werk, &range) {
        Ok(d) => d,
        Err(why) => {
            record_gate(home, role, card, gate, "error", &format!("no branch diff to review: {}", why), round);
            return;
        }
    };
    // #4889 (Kade 19:00) — a big diff is reviewed per file, in batches under the cap;
    // what cannot be read whole is unmeasured, never a pass (the 60k cut was hollow).
    let (batches, unread) = diff_batches(&diff, DIFF_BATCH_CAP);
    let n = batches.len();
    let mut parts: Vec<(String, String)> = Vec::new();
    for (i, batch) in batches.iter().enumerate() {
        let part = if n > 1 { format!(" (part {} of {})", i + 1, n) } else { String::new() };
        let ctx = format!(
            "CARD #{}\n{}\n\n=== BRANCH DIFF ({}){} ===\n{}",
            card, card_view, range, part, batch
        );
        parts.push(review_once(&sys, &ctx, round, &fail_on, role));
    }
    let (mut result, mut findings) = combine_verdicts(&parts, &unread);
    if strict {
        findings = format!("{} names no fail-items: every finding fails | {}", skill.display(), findings);
    }
    // #4889 (Jeff 19:04, "fine do that"): the code review IS Claude Code's /code-review,
    // with /security-review beside it; gate-code adds only what they do not read, whether
    // each AC's check tests that AC. They read the werk themselves, so no diff cap applies.
    // #4889 (Jeff 19:04) — Semgrep in the quality review, on the card's changed lines
    // #4889 (security review, run 9) — the rules and the boundary config are read from
    // canonical, like the skills: a card never rewrites the checks run on it. Until a
    // config is on main its review is unmeasured (error), never a pass.
    if gate == "quality" {
        let rules = Path::new(home).join("platform/quality/semgrep");
        let (r, f) = review_config_present(&rules).unwrap_or_else(|| semgrep_review(&werk, &rules, since.as_deref()));
        (result, findings) = combine_verdicts(&[(result.clone(), format!("gate-quality: {}", findings)), (r, format!("semgrep: {}", f))], &[]);
    }
    // #4889 (Jeff 19:04) — dependency-cruiser in the architecture review: the TS packages'
    // boundaries (platform/quality/depcruise/boundaries.cjs) hold on the whole tree
    if gate == "arch" {
        let cfg = Path::new(home).join("platform/quality/depcruise/boundaries.cjs");
        let (r, f) = review_config_present(&cfg).unwrap_or_else(|| depcruise_review(&werk, &cfg));
        (result, findings) = combine_verdicts(&[(result.clone(), format!("gate-arch: {}", findings)), (r, format!("dependency-cruiser: {}", f))], &[]);
    }
    if gate == "code" {
        let mut all = vec![(result.clone(), format!("gate-code: {}", findings))];
        for skill in BUILTIN_CODE_REVIEWS {
            let (r, f) = builtin_review(skill, &werk, since.as_deref());
            all.push((r, format!("/{}: {}", skill, f)));
        }
        // #4889 AC8 — an AC whose check is an unrelated test fails the code review
        let head_of = |p: &str| fs::read_to_string(Path::new(&werk).join(p)).ok()
            .map(|s| s.lines().take(15).collect::<Vec<_>>().join("\n"));
        let unrelated = unrelated_checks(&card_view, card, head_of);
        all.push(if unrelated.is_empty() { ("pass".to_string(), "ac-checks: no findings".to_string()) }
                 else { ("fail".to_string(), format!("ac-checks: {}", unrelated.join(" | "))) });
        (result, findings) = combine_verdicts(&all, &[]);
    }
    // #4889 — the findings ledger: an earlier round's open finding carries forward until a
    // later diff touches its file and does not raise it again (Kade 10:07). There is no
    // waiver: a finding leaves only by a fix (Kade 10:24 — a self-asserted settle is forgeable).
    (result, findings) = apply_ledger(&result, &findings, prior.as_ref().map(|(_, f)| f.as_str()), delta_files.as_deref());
    // #4889 (Jeff 16:05, "Yes, certainly") — only a finding that breaks something Jeff
    // uses fails the gate; every other finding is a note on his go page, and ships
    if result == "fail" {
        let open = open_findings(&findings);
        (result, findings) = apply_breaks(&result, &findings, classify_breaks(&werk, &open).as_deref());
    }
    // an unmeasured round reviewed nothing: the next round starts from the last one that did
    let reviewed = if result == "error" { "" } else { head.as_str() };
    record_gate_at(home, role, card, gate, &result, &findings, round, reviewed);
}

/// #4889 — where a round's review starts: the commit this gate last reviewed, else the
/// branch's merge-base with origin/main (the whole branch).
fn review_base(werk: &str, since: Option<&str>) -> Result<String, String> {
    if let Some(c) = since { return Ok(c.to_string()); }
    run_in_dir(werk, "git", &["merge-base", "origin/main", "HEAD"]).map(|b| b.trim().to_string())
        .map_err(|e| format!("no merge-base in {werk}: {e}"))
}

/// #4889 — the last recorded result of `gate` for `card` that names its commit:
/// (commit, findings). Pure over the witness text.
pub fn last_reviewed(witness: &str, card: u64, gate: &str) -> Option<(String, String)> {
    let card_key = format!("\"card_id\":{},", card);
    let gate_key = format!("\"gate\":\"{}\"", gate);
    witness.lines().rev()
        .filter(|l| l.contains("\"event\":\"demo.gate.result\"") && l.contains(&card_key) && l.contains(&gate_key))
        .find_map(|l| {
            let c = extract_json_str(l, "commit")?;
            (!c.is_empty()).then(|| (c, extract_json_str(l, "findings").unwrap_or_default()))
        })
}

/// #4889 — one finding's segment, with the labels and part markers a combined record
/// wraps it in taken off. None for a note, a meta line or "no findings". Pure.
fn fail_segment(seg: &str) -> Option<String> {
    let s = unwrapped(seg);
    if s.is_empty() || s.starts_with("note:") || s == "no findings" || s.contains("names no fail-items")
        || s.starts_with("unmeasured") { return None; }
    Some(s)
}

/// A segment with its part markers (`[part 1/3 fail] `) and review labels taken off. Pure.
fn unwrapped(seg: &str) -> String {
    let mut s = seg.trim();
    loop {
        let before = s;
        if let Some(r) = s.strip_prefix('[').and_then(|r| r.split_once("] ")).map(|(_, r)| r) { s = r.trim(); }
        // #4889 (code review, run 14) — a carried finding is still the finding: it carries
        // again next round until fixed, never dropped after one
        for label in ["carried (not yet fixed): ", "gate-code: ", "gate-arch: ", "gate-quality: ", "semgrep: ", "dependency-cruiser: ", "/code-review: ", "/security-review: ", "ac-checks: "] {
            if let Some(r) = s.strip_prefix(label) { s = r.trim(); }
        }
        if s == before { break; }
    }
    s.to_string()
}

/// #4889 — a finding's ledger key: (item, file). The item is the `item N` the review
/// named (0 when none); the file is what precedes the first `:` or ` — `. Pure.
pub fn finding_key(seg: &str) -> (u32, String) {
    let item = seg.split(" — ").find_map(|p| p.trim().strip_prefix("item ").and_then(|n| n.trim().parse().ok()))
        .or_else(|| seg.split("item ").nth(1).and_then(|r| r.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()))
        .unwrap_or(0);
    let head = seg.split(" — ").next().unwrap_or(seg);
    let file = head.split(':').next().unwrap_or(head).trim().to_string();
    (item, file)
}

/// #4889 (Jeff 11-05 10:06; Kade 10:07) — the ledger over one gate's record. `prior` is the
/// last round's findings, `delta` the files changed since it (None = a whole-branch round).
/// A prior failing finding whose file the delta left alone carries forward, red; one whose
/// file changed and is not raised again is fixed. An "error" record stays error. Pure.
pub fn apply_ledger(result: &str, findings: &str, prior: Option<&str>, delta: Option<&[String]>) -> (String, String) {
    // (as recorded, with its labels; cleaned, for its key)
    let fails = |text: &str| -> Vec<(String, String)> {
        text.split(" | ").filter_map(|s| fail_segment(s).map(|c| (s.trim().to_string(), c))).collect()
    };
    let now = fails(findings);
    let notes: Vec<String> = findings.split(" | ").filter(|s| fail_segment(s).is_none())
        .filter(|s| { let u = unwrapped(s); !u.is_empty() && u != "no findings" }).map(|s| s.trim().to_string()).collect();
    let changed = |f: &str| delta.is_some_and(|d| d.iter().any(|x| x == f));
    let now_keys: Vec<(u32, String)> = now.iter().map(|(_, c)| finding_key(c)).collect();
    let carried: Vec<(String, String)> = match (prior, delta) {
        (Some(p), Some(_)) => fails(p).into_iter()
            .filter(|(_, c)| { let k = finding_key(c); !changed(&k.1) && !now_keys.contains(&k) })
            .map(|(_, c)| (format!("carried (not yet fixed): {c}"), c))
            .collect(),
        _ => Vec::new(),
    };
    // #4889 (Kade 16:26) — a note stays on Jeff's go page until its file changes, like a red
    let carried_notes: Vec<String> = match (prior, delta) {
        (Some(p), Some(_)) => p.split(" | ").filter_map(|seg| unwrapped(seg).strip_prefix("note: ").map(str::to_string))
            .filter(|c| { let k = finding_key(c); !changed(&k.1) && !now_keys.contains(&k) })
            .map(|c| format!("note: {c}"))
            .filter(|n| !notes.iter().any(|x| unwrapped(x) == *n))
            .collect(),
        _ => Vec::new(),
    };
    let open: Vec<String> = now.into_iter().chain(carried).map(|(shown, _)| shown).collect();
    let verdict = if result == "error" { "error" } else if open.is_empty() { "pass" } else { "fail" };
    let mut out = open;
    out.extend(notes);
    out.extend(carried_notes);
    (verdict.to_string(), if out.is_empty() { "no findings".to_string() } else { out.join(" | ") })
}

/// #4889 (Jeff 16:05) — a red round's open findings, in order: the ones the breaks
/// question is asked about. Pure.
pub fn open_findings(findings: &str) -> Vec<String> {
    findings.split(" | ").filter_map(fail_segment).collect()
}

/// #4889 (Jeff 16:05: "breaks something Jeff uses → blocks the land; nothing breaks →
/// note on his go page, ships") — `breaks[i]` answers for the i-th open finding. Each
/// finding that breaks nothing becomes a `note:` (the ledger never carries a note); the
/// round fails only if one breaks. No answer, or one that does not match the findings,
/// leaves the round failed: an unclassified finding blocks. Pure.
pub fn apply_breaks(result: &str, findings: &str, breaks: Option<&[bool]>) -> (String, String) {
    let n = open_findings(findings).len();
    let Some(b) = breaks.filter(|b| b.len() == n && n > 0) else {
        return (result.to_string(), findings.to_string());
    };
    let mut i = 0usize;
    let out: Vec<String> = findings.split(" | ").map(|seg| match fail_segment(seg) {
        Some(clean) => {
            // i < n == b.len(), by the filter above
            let keep = b[i];
            i += 1;
            if keep { seg.trim().to_string() } else { format!("note: {clean}") }
        }
        None => seg.trim().to_string(),
    }).collect();
    let verdict = if b.iter().any(|x| *x) { "fail" } else { "pass" };
    (verdict.to_string(), out.join(" | "))
}

/// #4889 (Jeff 16:05) — what the breaks question asks, one finding per numbered line. Pure.
pub fn breaks_prompt(open: &[String], jeff_uses: &str) -> String {
    let list: Vec<String> = open.iter().enumerate().map(|(i, f)| format!("{}. {}", i + 1, f)).collect();
    format!("Jeff's rule for a card about to land: a review finding blocks the land only if it breaks \
             something Jeff uses: {jeff_uses}. Every other finding is a note on his go page and ships. \
             For each numbered finding below, read the code it names in this repo, then answer true if \
             it breaks something Jeff uses, else false. Reply with only a ```json list of booleans, \
             one per finding, in order.\n\n{}",
            list.join("\n"))
}

/// #4889 (Kade 16:26) — what Jeff uses is a policy row (review.breaks.jeff_uses), read
/// through the resolver; no row, or the door down, means no question is asked and the
/// red stands.
pub const JEFF_USES_KEY: &str = "review.breaks.jeff_uses";

/// The ```json list of booleans a breaks answer ends with; None when there is none. Pure.
pub fn breaks_answer(stdout: &str) -> Option<Vec<bool>> {
    let start = stdout.rfind("```json")? + 7;
    let body = &stdout[start..];
    let body = &body[..body.find("```")?];
    let inner = body.trim().strip_prefix('[')?.strip_suffix(']')?;
    inner.split(',').map(str::trim).filter(|t| !t.is_empty()).map(|t| match t {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }).collect()
}

/// One headless question per red round: which open findings break something Jeff uses.
/// No tools, user settings only; None when it could not be asked or answered.
fn classify_breaks(werk: &str, open: &[String]) -> Option<Vec<bool>> {
    if open.is_empty() { return None; }
    let jeff_uses = properties_resolver::global_value(&properties_resolver::door_url(), JEFF_USES_KEY)
        .map_err(|e| eprintln!("werk-demo: no breaks question asked, the red stands: {e}")).ok()?;
    let bin = claude_bin()?;
    let mut command = Command::new(&bin);
    // in the card's werk, read-only, so it reads the code each finding names (Kade 16:26)
    command.current_dir(werk).env("CHORUS_HEADLESS", "1").args(["-p", &breaks_prompt(open, &jeff_uses), "--permission-mode", "dontAsk",
        "--setting-sources", "user", "--no-session-persistence", "--output-format", "text",
        "--allowedTools", "Read,Grep,Glob", "--disallowedTools", REVIEW_DENIED_TOOLS]);
    // the answer goes to the step's log either way, so a red that stood can be read (run 15)
    // run 15: the question reads the code each finding names; 12 arch findings took ~8 min,
    // past a 300s bound, and the red stood. It gets the reviews' own bound.
    match bounded_headless(command, String::new(), review_timeout_secs()) {
        Ok(out) => {
            let a = breaks_answer(&out);
            eprintln!("werk-demo: breaks question on {} finding(s) answered {:?}{}", open.len(), a,
                if a.as_ref().is_some_and(|v| v.len() == open.len()) { String::new() } else { format!(" — unusable, the red stands: {}", out.trim().chars().take(300).collect::<String>()) });
            a
        }
        Err(e) => { eprintln!("werk-demo: breaks question not answered, the red stands: {e}"); None }
    }
}

/// #4889 — what `claude -p` is asked for each built-in review. /code-review runs as its
/// slash command. /security-review is a local command under `-p` (measured 11-05: zero
/// turns, empty result), so the model is asked to invoke the security-review skill
/// through the Skill tool, the same skill, never a copy of its text (Kade 09:16). Both
/// end in the ```json findings list builtin_review_verdict reads. Pure.
pub fn builtin_review_prompt(skill: &str, base: &str) -> String {
    if skill == "code-review" {
        format!("/code-review medium --max-findings all {base}...HEAD")
    } else {
        format!("Invoke the {skill} skill on the changes {base}...HEAD of this branch. When it is done, end your reply \
                 with a ```json block holding a list of its findings, each {{\"file\":\"...\",\"line\":N,\"summary\":\"...\"}}, \
                 and [] when there are none.")
    }
}

/// #4889 (Kade 15:12) — the read-only git verbs a review may run, minus their one write:
/// `--output=<file>`.
pub const REVIEW_DENIED_TOOLS: &str = "Bash(git diff *--output*),Bash(git log *--output*),Bash(git show *--output*)";

/// #4889 (security review, run 12) — the argv of one built-in review. The card under
/// review is the cwd, so its own .claude/settings.json (hooks, permission allows) must not
/// load: only user settings do (`--setting-sources user`, where the review skills live).
/// Every review gets read-only tools; the code review also its subagents. Pure.
pub fn builtin_review_args(skill: &str, prompt: &str) -> Vec<String> {
    let tools = if skill == "code-review" { "Agent,Read,Grep,Glob,Bash(git diff:*),Bash(git log:*),Bash(git show:*)" }
                else { "Skill,Read,Grep,Glob,Bash(git diff:*),Bash(git log:*),Bash(git show:*)" };
    // git diff/log/show write a file with --output=<path> (Kade 15:12): denied, a deny wins over an allow
    ["-p", prompt, "--permission-mode", "dontAsk", "--setting-sources", "user", "--no-session-persistence",
     "--output-format", "text", "--allowedTools", tools, "--disallowedTools", REVIEW_DENIED_TOOLS].iter().map(|a| a.to_string()).collect()
}

/// #4889 — how long one headless review (or the breaks question about its findings) may
/// run: CHORUS_CODE_REVIEW_TIMEOUT_SECS, default 900.
pub fn review_timeout_secs() -> u64 {
    env::var("CHORUS_CODE_REVIEW_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(900)
}

/// #4889 — the existing Claude Code reviews the code step runs, each on the whole branch.
pub const BUILTIN_CODE_REVIEWS: [&str; 2] = ["code-review", "security-review"];

/// #4889 — run one built-in review headless in the card's werk over `origin/main...HEAD`,
/// bounded (CHORUS_CODE_REVIEW_TIMEOUT_SECS, default 900). It reads the repo with its own
/// tools, so the whole diff is read; ("error", why) when it could not run.
fn builtin_review(skill: &str, werk: &str, since: Option<&str>) -> (String, String) {
    let Some(bin) = claude_bin() else { return ("error".to_string(), "claude binary not resolvable".to_string()) };
    let base = match review_base(werk, since) {
        Ok(b) => b,
        Err(e) => return ("error".to_string(), e),
    };
    let prompt = builtin_review_prompt(skill, &base);
    let timeout = review_timeout_secs();
    let mut command = Command::new(&bin);
    command.current_dir(werk).env("CHORUS_HEADLESS", "1")
        .args(builtin_review_args(skill, &prompt));
    match bounded_headless(command, String::new(), timeout) {
        Ok(out) => builtin_review_verdict(&out),
        Err(e) => ("error".to_string(), e.replace("chorus-agent", &format!("/{skill}"))),
    }
}

/// #4889 — the quality rules (platform/quality/semgrep) over the lines the card changed
/// (`--baseline-commit` = its merge-base). Not installed, or no readable report, is an
/// error: unmeasured, never a pass.
fn semgrep_review(werk: &str, rules: &Path, since: Option<&str>) -> (String, String) {
    let base = match review_base(werk, since) {
        Ok(b) => b,
        Err(e) => return ("error".to_string(), e),
    };
    // #4889 (Jeff 20:55) — semgrep's path is the tool.path.semgrep property, read through the
    // resolver: the door down or the row missing is an error (unmeasured), never a default
    let bin = match properties_resolver::global_value(&properties_resolver::door_url(), "tool.path.semgrep") {
        Ok(b) => b,
        Err(e) => return ("error".to_string(), format!("semgrep: {e}")),
    };
    let out = Command::new(&bin).current_dir(werk)
        // a file CA bundle: without it semgrep asks the keychain and aborts (measured 11-04)
        .env("SSL_CERT_FILE", env::var("SSL_CERT_FILE").unwrap_or_else(|_| "/etc/ssl/cert.pem".to_string()))
        .args(["scan", "--config"]).arg(rules)
        .args(["--baseline-commit", &base, "--json", "--metrics=off", "--disable-version-check", "--exclude", "fixtures"])
        .output();
    match out {
        Ok(o) => semgrep_verdict(&String::from_utf8_lossy(&o.stdout)),
        Err(e) => ("error".to_string(), format!("semgrep could not run: {e}")),
    }
}

/// #4889 — the one resolver, compiled in (zero-dep blueprint: a path module, no crate dep).
#[allow(dead_code)]
#[path = "../../properties-resolver/src/lib.rs"]
mod properties_resolver;

/// #4889 — the TS packages dependency-cruiser checks (the boundary rule's own list).
pub const DEPCRUISE_DIRS: [&str; 7] = ["directing", "platform/api", "platform/pulse", "platform/workflow-engine",
    "platform/agent-adapters", "platform/chorus-sdk", "roles/wren/scripts"];

/// #4889 — a review whose config (from canonical) is missing is an error with that
/// reason; None when it is there. Pure on the filesystem state it reads.
pub fn review_config_present(cfg: &Path) -> Option<(String, String)> {
    (!cfg.exists()).then(|| ("error".to_string(), format!("{} is not on canonical: this review is unmeasured until it lands", cfg.display())))
}

fn depcruise_review(werk: &str, cfg: &Path) -> (String, String) {
    let dirs: Vec<&str> = DEPCRUISE_DIRS.iter().copied().filter(|d| Path::new(werk).join(d).is_dir()).collect();
    let out = Command::new(env::var("CHORUS_NPX_BIN").unwrap_or_else(|_| "npx".to_string())).current_dir(werk)
        .args(["--no-install", "depcruise", "--config"]).arg(cfg).args(["--output-type", "err"])
        .args(&dirs).output();
    match out {
        Ok(o) => depcruise_verdict(&format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)), o.status.code()),
        Err(e) => ("error".to_string(), format!("dependency-cruiser could not run: {e}")),
    }
}

/// #4889 — dependency-cruiser's `err` report to a verdict: each `error <rule>: a → b`
/// line fails; a clean summary passes; anything else (not installed, a crash) is an
/// error, never a pass. Pure.
pub fn depcruise_verdict(out: &str, code: Option<i32>) -> (String, String) {
    let errs: Vec<String> = out.lines().map(str::trim).filter(|l| l.starts_with("error ")).map(|l| l[6..].to_string()).collect();
    if !errs.is_empty() { return ("fail".to_string(), errs.join(" | ")); }
    if code == Some(0) && out.contains("no dependency violations found") { return ("pass".to_string(), "no findings".to_string()); }
    ("error".to_string(), format!("no dependency-cruiser report (exit {:?}): {}", code, out.trim().chars().take(160).collect::<String>()))
}

/// #4889 — Semgrep's JSON report to a verdict: an ERROR-severity result fails, others
/// are notes, none passes; a report with no results list is an error. Pure.
pub fn semgrep_verdict(json: &str) -> (String, String) {
    let Some(objs) = array_objects(json, "results") else {
        return ("error".to_string(), format!("no semgrep report: {}", json.chars().take(160).collect::<String>()));
    };
    let (mut fails, mut notes) = (Vec::new(), Vec::new());
    for o in &objs {
        let path = extract_json_str(o, "path").map(|v| json_unescape(&v)).unwrap_or_default();
        let line = o.find("\"line\"").map(|k| o[k + 6..].trim_start().trim_start_matches(':').trim_start()
            .chars().take_while(|c| c.is_ascii_digit()).collect::<String>()).unwrap_or_default();
        let msg = extract_json_str(o, "message").map(|v| json_unescape(&v)).unwrap_or_default();
        let entry = format!("{path}:{line} — {msg}");
        if extract_json_str(o, "severity").as_deref() == Some("ERROR") { fails.push(entry) } else { notes.push(format!("note: {entry}")) }
    }
    let result = if fails.is_empty() { "pass" } else { "fail" };
    fails.extend(notes);
    (result.to_string(), if fails.is_empty() { "no findings".to_string() } else { fails.join(" | ") })
}

fn run_in_dir(dir: &str, cmd: &str, args: &[&str]) -> R<String> {
    let o = Command::new(cmd).current_dir(dir).args(args).output().map_err(|e| format!("{cmd}: {e}"))?;
    if !o.status.success() { return Err(String::from_utf8_lossy(&o.stderr).trim().to_string()); }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// #4889 — a built-in review's verdict, derived from its findings (Kade 19:19): the reply
/// ends in a JSON list of findings ({file, line, summary, ...}); any finding fails, an
/// empty list passes, no list is an error (never a pass). Pure + unit-tested.
pub fn builtin_review_verdict(stdout: &str) -> (String, String) {
    let Some(open) = stdout.rfind("```json").map(|i| i + 7).and_then(|i| stdout[i..].find('[').map(|j| i + j)) else {
        return ("error".to_string(), format!("no findings list in the review: {}", stdout.chars().take(160).collect::<String>()));
    };
    let wrapped = format!("{{\"findings\":{}", &stdout[open..]);
    let Some(objs) = findings_objects(&wrapped) else {
        return ("error".to_string(), "the findings list never closes".to_string());
    };
    let lines: Vec<String> = objs.iter().map(|o| {
        let file = extract_json_str(o, "file").map(|v| json_unescape(&v)).unwrap_or_default();
        let line = o.find("\"line\"").map(|k| o[k + 6..].trim_start().trim_start_matches(':').trim_start()
            .chars().take_while(|c| c.is_ascii_digit()).collect::<String>()).unwrap_or_default();
        let what = extract_json_str(o, "summary").map(|v| json_unescape(&v)).unwrap_or_default();
        format!("{file}:{line} — {what}")
    }).collect();
    if lines.is_empty() { ("pass".to_string(), "no findings".to_string()) } else { ("fail".to_string(), lines.join(" | ")) }
}

/// #4889 — the largest diff one review reads at once.
pub const DIFF_BATCH_CAP: usize = 60_000;

/// #4889 — split a diff into per-file pieces (a file over the cap into its hunks, each
/// under its file header) and pack them into batches of at most `cap` chars. A single
/// hunk over the cap cannot be read whole: it is returned in the second list, by file.
pub fn diff_batches(diff: &str, cap: usize) -> (Vec<String>, Vec<String>) {
    let mut files: Vec<String> = Vec::new();
    for line in diff.split_inclusive('\n') {
        if line.starts_with("diff --git ") || files.is_empty() { files.push(String::new()); }
        files.last_mut().unwrap().push_str(line);
    }
    let mut pieces: Vec<String> = Vec::new();
    let mut unread: Vec<String> = Vec::new();
    for f in files.into_iter().filter(|f| !f.is_empty()) {
        if f.chars().count() <= cap { pieces.push(f); continue; }
        let name = f.lines().next().unwrap_or("").trim_start_matches("diff --git ").to_string();
        let (head, hunks) = match f.find("\n@@") { Some(i) => f.split_at(i + 1), None => (f.as_str(), "") };
        let mut hunk_list: Vec<String> = Vec::new();
        for line in hunks.split_inclusive('\n') {
            if line.starts_with("@@") || hunk_list.is_empty() { hunk_list.push(String::new()); }
            hunk_list.last_mut().unwrap().push_str(line);
        }
        if hunk_list.is_empty() { unread.push(name); continue; }
        for h in hunk_list {
            let piece = format!("{}{}", head, h);
            if piece.chars().count() > cap { if !unread.contains(&name) { unread.push(name.clone()); } } else { pieces.push(piece); }
        }
    }
    let mut batches: Vec<String> = Vec::new();
    for p in pieces {
        match batches.last_mut() {
            Some(b) if b.chars().count() + p.chars().count() <= cap => b.push_str(&p),
            _ => batches.push(p),
        }
    }
    if batches.is_empty() && unread.is_empty() { batches.push(String::new()); }
    (batches, unread)
}

/// #4889 — one verdict from the batches' verdicts: a part that could not run, or a part
/// of the diff that could not be read, is an error (unmeasured, never a pass); else any
/// fail fails; else pass. Every part's findings are kept, labelled.
pub fn combine_verdicts(parts: &[(String, String)], unread: &[String]) -> (String, String) {
    let n = parts.len();
    let mut notes: Vec<String> = parts.iter().enumerate()
        .filter(|(_, (_, f))| !f.trim().is_empty())
        .map(|(i, (r, f))| if n > 1 { format!("[part {}/{} {}] {}", i + 1, n, r, f) } else { f.clone() })
        .collect();
    if !unread.is_empty() {
        notes.insert(0, format!("unmeasured: too big to read whole: {}", unread.join(", ")));
    }
    let result = if !unread.is_empty() || parts.iter().any(|(r, _)| r == "error") { "error" }
        else if parts.iter().any(|(r, _)| r == "fail") { "fail" } else { "pass" };
    (result.to_string(), notes.join(" | "))
}

/// One headless review of one context; ("error", why) when it could not run.
/// #4921 — a reviewer that exits non-zero: its whole stdout and stderr go to
/// the step log, and the finding carries the LAST part of each, where the real
/// error sits. On 11-07 the first 160 chars of stderr were a settings warning;
/// with all of stderr kept, the warning was ALL of it (12:33): claude -p prints
/// its error on stdout, which the finding used to drop.
pub fn crashed_reviewer(who: &str, how: &str, stderr: &str, stdout: &str) -> String {
    eprintln!("werk-demo: {who} {how} — full stdout follows ({} bytes)\n{stdout}", stdout.len());
    eprintln!("werk-demo: {who} {how} — full stderr follows ({} bytes)\n{stderr}", stderr.len());
    format!("{who} {how}: stdout: {} · stderr: {}", tail(stdout), tail(stderr))
}

/// The last 400 chars of a stream, blank lines dropped, lines joined by " | ";
/// "(empty)" when there was nothing, "… " in front when it was cut.
fn tail(text: &str) -> String {
    const TAIL: usize = 400;
    let flat = text.trim().lines().filter(|l| !l.trim().is_empty()).collect::<Vec<_>>().join(" | ");
    if flat.is_empty() {
        return "(empty)".to_string();
    }
    let n = flat.chars().count();
    let last: String = flat.chars().skip(n.saturating_sub(TAIL)).collect();
    format!("{}{last}", if n > TAIL { "… " } else { "" })
}

/// One review by claude -p (or chorus-agent when CHORUS_GATE_PROFILE is set):
/// (verdict, finding). Public for the #4921 fake-claude test.
/// #4921 (Jeff 12:53: "run as one of u / why am i needed") — the gate's claude
/// runs as the card's role account, under that role's own login, never under
/// the runner's (jeffbridwell's login expired at 12:48 and every demo crashed).
/// The hop is the one chorus-principal starts role panes with (#4777), covered
/// by /etc/sudoers.d/chorus-principals. Already that account: no hop. Returns
/// (program, args, who) — `who` names the account in a crash finding.
pub fn gate_command(me: &str, role: &str, bin: &str, model: &str, sys: &str) -> (String, Vec<String>, String) {
    let claude: Vec<String> = [
        bin, "-p", "--model", model, "--permission-mode", "dontAsk", "--no-session-persistence",
        "--output-format", "text", "--disallowedTools", "Bash,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task",
        "--system-prompt", sys,
    ].iter().map(|s| s.to_string()).collect();
    let account = format!("chorus-{role}");
    if role.is_empty() || me == account {
        return (claude[0].clone(), claude[1..].to_vec(), format!("claude as {me}"));
    }
    let quoted: Vec<String> = claude.iter().map(|a| format!("'{}'", a.replace('\'', "'\\''"))).collect();
    let inner = format!(
        // the role's own sign-in (~/.claude under -H); never ~/.chorus/secrets/claude.key,
        // whose Oct 28 token was revoked (13:02 run: "OAuth token revoked")
        "export CHORUS_HEADLESS=1 CHORUS_ROLE='{role}' && exec {}",
        quoted.join(" ")
    );
    ("sudo".to_string(), ["-n", "-u", &account, "-H", "bash", "-c", &inner].iter().map(|s| s.to_string()).collect(), format!("claude as {account}"))
}

pub fn review_once(sys: &str, ctx: &str, round: &str, fail_on: &[u32], role: &str) -> (String, String) {
    if let Some(job) = gate_via_agent(ctx, sys, GATE_OUTPUT_SCHEMA, round) {
        return match job {
            Ok(out) => gate_verdict(&out, fail_on),
            Err(e) => ("error".to_string(), e),
        };
    }
    let model = env::var("CHORUS_GATE_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
    let bin = match claude_bin() {
        Some(b) => b,
        None => {
            return ("error".to_string(), ("claude binary not resolvable").to_string());
        }
    };
    let me = env::var("USER").unwrap_or_default();
    let (program, args, who) = gate_command(&me, role, &bin, &model, sys);
    // #3471 — CHORUS_HEADLESS marks the gate's claude -p as headless so
    // chorus-hooks' RESPOND-FIRST block exempts it (also set inside the hop,
    // since sudo drops the environment).
    let child = Command::new(&program)
        .env("CHORUS_HEADLESS", "1")
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            return ("error".to_string(), format!("{who} spawn failed: {e}"));
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(ctx.as_bytes());
        // stdin drops here → closed, so claude sees EOF and proceeds.
    }
    // #3443 (Silas's BOUNDED contract) — a hung model must NOT block the gate
    // forever. Poll for exit up to CHORUS_GATE_TIMEOUT_SECS (default 600; 180 timed out on 3 of 5 past diffs, #4889); on
    // expiry, kill the child and record error-on-expiry (fail-LOUD, never green).
    // #4889 — 180s timed out on 3 of 5 past diffs (one only 18k): 600s
    let timeout_secs: u64 = env::var("CHORUS_GATE_TIMEOUT_SECS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(600);
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if start.elapsed() >= Duration::from_secs(timeout_secs) {
                    // #4921 — through the hop the child is sudo: SIGTERM is
                    // relayed to claude, SIGKILL would orphan it, so TERM first
                    unsafe { kill(child.id() as i32, 15); }
                    sleep(Duration::from_secs(2));
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                sleep(Duration::from_millis(200));
            }
            Err(e) => {
                return ("error".to_string(), (format!("claude wait failed: {}", e)).to_string());
            }
        }
    };
    let status = match status {
        Some(s) => s,
        None => {
            return ("error".to_string(), (format!("gate timed out after {}s — claude killed (bounded, fail-loud)", timeout_secs)).to_string());
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
        return ("error".to_string(), crashed_reviewer(&who, &format!("exit {:?}", status.code()), &stderr, &stdout));
    }
    gate_verdict(&stdout, fail_on)
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
pub const DEMO_STEPS: [&str; 3] = ["validate", "run-gate", "announce"];

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
/// The demo steps (#4502, #4889) — `validate`, `run-gate <code|arch|quality>`,
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
            _ => announce_step(card, role, &home),
        };
    }

    if first == "vouch" {
        // #4875 AC2 — the ONLY way a go reaches Jeff. The owner vouches (shown / proven / not
        // proven), the navigator vouches (why); once every gate passed and both are on the
        // witness for this round, the go row is posted and Jeff gets one message. The voucher
        // is the verified caller (the OS account), never an argument.
        let usage_v = "usage: werk-demo vouch <card> owner --shown <url> --proven <text> [--not-proven <text>] | werk-demo vouch <card> navigator --why <text>";
        let card: u64 = args.next().and_then(|s| s.parse().ok()).ok_or(usage_v)?;
        let as_ = args.next().filter(|a| a == "owner" || a == "navigator").ok_or(usage_v)?;
        let rest: Vec<String> = args.collect();
        let opt = |k: &str| rest.iter().position(|a| a == k).and_then(|i| rest.get(i + 1)).cloned().unwrap_or_default();
        let by = verified_caller().ok_or("vouch refused: the caller is not a role account (chorus-<role>)")?;
        let witness_path = home.join("ops/logs/werk-demo.jsonl");
        let witness = fs::read_to_string(&witness_path).unwrap_or_default();
        let card_key = format!("\"card_id\":{},", card);
        let announced = witness.lines().rev()
            .find(|l| l.contains("\"event\":\"demo.announced\"") && l.contains(&card_key))
            .ok_or_else(|| format!("vouch refused: #{card} has no demo announced yet"))?;
        let owner = json_str_after(announced, "role").unwrap_or_default();
        let run_id = json_str_after(announced, "trace_id").unwrap_or_default();
        if as_ == "owner" && by != owner { return Err(format!("vouch refused: only the owner ({owner}) vouches as owner, not {by}")); }
        if as_ == "navigator" && by == owner { return Err(format!("vouch refused: {by} owns #{card} and cannot be its navigator")); }
        let round = current_round(&owner, card);
        let patch = current_patch_id(&owner, card);
        let line = vouch_line(card, &by, &as_, &round, &patch, &opt("--shown"), &opt("--proven"), &opt("--not-proven"), &opt("--why"));
        { use std::io::Write as _; let mut f = fs::OpenOptions::new().create(true).append(true).open(&witness_path).map_err(|e| e.to_string())?; f.write_all(line.as_bytes()).map_err(|e| e.to_string())?; }
        let witness = fs::read_to_string(&witness_path).unwrap_or_default();
        if go_already_asked(&witness, card, &round, &patch) {
            return Ok(DemoOutcome { message: format!("{as_} vouch recorded for #{card}; the go already reached Jeff for this round — nothing more sent"), exit: 0 });
        }
        // one reader of the card's AC: borg-prove's (Jeff 09:28); a failed read is "" → refused
        let exe = env::current_exe().ok();
        let bp = borg_prove_beside(exe.as_deref().and_then(Path::parent), script_path(&home, "borg-prove"));
        let cv = run(&bp, &["ac", &card.to_string()]).unwrap_or_default();
        let mut missing = vouch_missing(&witness, card, &round, &patch, &owner);
        missing.extend(ac_unfinished(&cv));
        if !missing.is_empty() {
            return Ok(DemoOutcome { message: format!("{as_} vouch recorded for #{card}; not sent to Jeff yet — missing: {}", missing.join(", ")), exit: 0 });
        }
        // Kade 09:36 — the go row is posted as the owner, so only the owner's own call posts it
        if !may_post_go(&by, &owner) {
            // Kade 09:40 — never a silent stall: tell the owner the go is theirs to post
            let a = nudge_send_args(&by, &owner, &owner_post_nudge(card));
            let argv: Vec<&str> = a.iter().map(String::as_str).collect();
            let _ = run(&hook_shim(), &argv);
            return Ok(DemoOutcome { message: format!("{as_} vouch recorded for #{card}; everything is in — {owner} was told to post the go with their own vouch"), exit: 0 });
        }
        let head = tested_head(&home, card, &run_id).ok_or_else(|| format!("no go row: no push of #{card} recorded for run {run_id}"))?;
        go_approval_body(card, &run_id, &head, &go_row_title(&home, card))
            .and_then(|body| post_go_approval(&home, &owner, &body))
            .map_err(|why| format!("vouches complete but the go row was refused: {why}"))?;
        let msg = jeff_go_message(&witness, card, &round, &patch, &owner, &approvals_link(), &ac_checklist(&cv));
        let a = nudge_send_args(&owner, "jeff", &msg);
        let argv: Vec<&str> = a.iter().map(String::as_str).collect();
        let _ = run(&hook_shim(), &argv);
        jsonl(&home, &owner, card, &run_id, "demo.go.asked", &format!(",\"round\":\"{}\",\"patch_id\":\"{}\"", round, patch));
        return Ok(DemoOutcome { message: format!("#{card}: both vouched, every gate passed — the go is on Jeff's list and he has one message."), exit: 0 });
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
        // verdict_proven — the reviews + Jeff's go, all patch-keyed for THIS
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
                format!("[verdict] #{} PROVEN (round {}) — reviews + Jeff's go all on the witness for this patch → land.", card, round)
            } else {
                let mut missing: Vec<&str> = Vec::new();
                if !announce_ready_full(&witness, card, role.trim(), &round, &patch, false) { missing.push("reviews"); }
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
         validate · run-gate <code|arch|quality> · announce; \
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
    fn fail_items_reads_ranges_and_singles_from_frontmatter() {
        let md = "---\nname: gate-x\nfail-items: 1-3, 7, 9\n---\n# x\nfail-items: 99\n";
        assert_eq!(fail_items(md), vec![1, 2, 3, 7, 9]);
        assert!(fail_items("---\nname: y\n---\n# y").is_empty(), "no rule → empty (the runner then fails every finding)");
    }

    #[test]
    fn the_security_review_is_asked_through_its_skill_not_the_local_slash_command_4889() {
        let p = builtin_review_prompt("security-review", "abc");
        assert!(p.starts_with("Invoke the security-review skill on the changes abc...HEAD"), "{p}");
        assert!(!p.starts_with('/'), "a slash command is a zero-turn local command under -p: {p}");
        assert!(p.contains("```json"), "{p}");
        assert_eq!(builtin_review_prompt("code-review", "abc"), "/code-review medium --max-findings all abc...HEAD");
    }

    #[test]
    fn a_review_config_missing_from_canonical_is_unmeasured_never_a_pass_4889() {
        let (r, f) = review_config_present(Path::new("/no/such/boundaries.cjs")).expect("missing is an answer");
        assert_eq!(r, "error");
        assert!(f.contains("not on canonical"), "{f}");
        assert!(review_config_present(Path::new("/")).is_none());
    }

    #[test]
    fn gate_verdict_derives_fail_from_a_fail_item_finding() {
        let out = r#"{"findings":[{"item":2,"where":"a.rs:3","what":"swallowed error"},{"item":6,"where":"b.rs:1","what":"no log"}]}"#;
        let (r, f) = gate_verdict(out, &[1, 2, 3]);
        assert_eq!(r, "fail");
        assert!(f.starts_with("a.rs:3 — item 2 — swallowed error"), "{f}");
        assert!(f.contains("note: b.rs:1 — item 6"), "{f}");
    }

    #[test]
    fn negative_proof_the_models_own_pass_word_is_ignored() {
        // #4708's shape: the reply says pass but lists a fail-item finding
        let out = r#"{"result":"pass","findings":[{"item":1,"where":"t.bats:9","what":"negative proof passes vacuously"}]}"#;
        assert_eq!(gate_verdict(out, &[1]).0, "fail");
    }

    #[test]
    fn notes_only_and_empty_lists_pass() {
        assert_eq!(gate_verdict(r#"{"findings":[{"item":6,"where":"x","what":"y"}]}"#, &[1]).0, "pass");
        assert_eq!(gate_verdict(r#"{"findings":[]}"#, &[1]), ("pass".to_string(), "no findings".to_string()));
        // #4889 (Kade 09:14) — strict (no fail-items): every finding fails, any item number
        assert_eq!(gate_verdict(r#"{"findings":[{"item":100,"where":"x","what":"y"}]}"#, &[]).0, "fail");
        assert_eq!(gate_verdict(r#"{"findings":[{"item":0,"where":"x","what":"y"}]}"#, &[]).0, "fail");
        assert_eq!(gate_verdict(r#"{"findings":[]}"#, &[]).0, "pass");
        // NP: a negative item is no item number at all — an error, never a pass
        assert_eq!(gate_verdict(r#"{"findings":[{"item":-1,"where":"x","what":"y"}]}"#, &[]).0, "error");
    }

    #[test]
    fn gate_verdict_unwraps_json_envelope() {
        let out = r#"{"type":"result","result":"{\"findings\":[{\"item\":3,\"where\":\"z.rs:2\",\"what\":\"null means two things\"}]}","is_error":false}"#;
        assert_eq!(gate_verdict(out, &[3]).0, "fail");
    }

    #[test]
    fn negative_proof_garbled_or_itemless_is_error_never_pass() {
        assert_eq!(gate_verdict("I think this looks fine to me, shipping it", &[1]).0, "error");
        assert_eq!(gate_verdict(r#"{"result":"pass","findings":"clean"}"#, &[1]).0, "error", "the old string shape is no list");
        assert_eq!(gate_verdict(r#"{"findings":[{"where":"a","what":"b"}]}"#, &[1]).0, "error");
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
        assert!(p.contains("\"findings\":[{\"item\""));
        assert!(p.contains("ONLY a single JSON object") && p.contains("Do not give a verdict"));
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
    fn gates_missing_empty_when_every_review_recorded() {
        let w = REQUIRED_GATES
            .iter()
            .map(|g| gate_line(3237, g))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(gates_missing(&w, 3237, "r1", "").is_empty(), "every review present → none missing");
    }

    #[test]
    fn gates_missing_lists_absent_gates() {
        let w = gate_line(3237, "code");
        assert_eq!(gates_missing(&w, 3237, "r1", ""), vec!["arch", "quality"]);
    }

    #[test]
    fn gates_missing_all_when_witness_empty() {
        assert_eq!(gates_missing("", 3237, "r1", "").len(), REQUIRED_GATES.len());
    }

    // === #3461 — gate/gather evidence survives a content-preserving rebase ===


    #[test]
    fn gates_survive_round_churn_via_patch_id() {
        let mut w = String::new();
        for g in REQUIRED_GATES {
            w.push_str(&format!("{{\"event\":\"demo.gate.result\",\"card_id\":3461,\"gate\":\"{}\",\"round\":\"rA\",\"patch_id\":\"pidX\"}}\n", g));
        }
        assert!(gates_missing(&w, 3461, "rB", "pidX").is_empty(), "gates survive rebase via patch-id");
        assert_eq!(gates_missing(&w, 3461, "rB", "pidY").len(), REQUIRED_GATES.len(), "real content change → gates re-run");
        assert_eq!(gates_missing(&w, 3461, "rB", "").len(), REQUIRED_GATES.len(), "empty patch → round-only");
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


    #[test]
    fn announce_full_fires_when_every_review_recorded() {
        // #4889: the reviews are the demo's only gate (peer gathers retired).
        let lines: Vec<String> = REQUIRED_GATES.iter().map(|g| gate_line(3352, g)).collect();
        assert!(announce_ready_full(&lines.join("\n"), 3352, "wren", "r1", "", false), "every review ⇒ announce");
        // negative proof: one review missing ⇒ no announce
        assert!(!announce_ready_full(&lines[..2].join("\n"), 3352, "wren", "r1", "", false), "a missing review blocks announce");
    }




    // #3365 — THE core proof: a prior round's records never satisfy this round.
    #[test]
    fn round2_with_only_round1_records_refuses_everything() {
        let lines: Vec<String> = REQUIRED_GATES.iter().map(|g| gate_line(3365, g)).collect();
        let w = lines.join("\n"); // a fully-passed ROUND r1
        assert!(announce_ready_full(&w, 3365, "wren", "r1", "", false), "r1 evidence satisfies r1");
        assert!(!announce_ready_full(&w, 3365, "wren", "r2", "", false), "r1 evidence must NOT satisfy r2");
        assert_eq!(gates_missing(&w, 3365, "r2", "").len(), REQUIRED_GATES.len(), "all reviews fresh per round");
    }

    #[test]
    fn pre_3365_records_without_round_field_match_nothing() {
        // Old evidence (no round field) ages out structurally on landing.
        let w = r#"{"ts":1,"event":"demo.gate.result","role":"wren","card_id":3365,"trace_id":"t","gate":"product","result":"pass"}"#;
        assert_eq!(gates_missing(w, 3365, "r1", "").len(), REQUIRED_GATES.len());
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
    fn gates_missing_ignores_other_cards_comma_terminated() {
        // a full gate set for card 31 must NOT satisfy card 3 (anti #3/#31 collision).
        let w = REQUIRED_GATES
            .iter()
            .map(|g| gate_line(31, g))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(gates_missing(&w, 3, "r1", "").len(), REQUIRED_GATES.len(), "card 31's gates must not satisfy card 3");
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
            gate_line_result(3284, "arch", "pass"),
            gate_line_result(3284, "code", "fail")
        );
        let map: std::collections::HashMap<_, _> = gate_verdicts(&w, 3284).into_iter().collect();
        assert_eq!(map["arch"], "pass");
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
            gate_line_result(3284, "arch", "pass"),
            gate_line_result(3284, "code", "fail")
        );
        let s = render_gate_summary(&w, 3284);
        assert!(s.contains("arch ✓"), "pass → ✓: {}", s);
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
        let bin = fake_agent(&dir, r#"{"findings":[{"item":8,"where":"a.rs:1","what":"missing test"}]}"#);
        env::set_var("CHORUS_GATE_PROFILE", "gate-chat");
        env::set_var("CHORUS_AGENT_BIN", &bin);
        let out = gate_via_agent("card text \"quoted\"", "gate sys", GATE_OUTPUT_SCHEMA, "abc123").expect("profile set").unwrap();
        env::remove_var("CHORUS_GATE_PROFILE");
        env::remove_var("CHORUS_AGENT_BIN");
        assert_eq!(gate_verdict(&out, &[8]).0, "fail");
        assert_eq!(fs::read_to_string(dir.join("args")).unwrap().trim(), "run --text");
        let stdin = fs::read_to_string(dir.join("stdin")).unwrap();
        assert_eq!(extract_json_str(&stdin, "profile").as_deref(), Some("gate-chat"));
        assert_eq!(json_unescape(&extract_json_str(&stdin, "input").unwrap()), "card text \"quoted\"");
        assert_eq!(extract_json_str(&stdin, "input_revision").as_deref(), Some("abc123"));
        assert!(stdin.contains("\"output_schema\":{"));
    }

    const PROFILES_ONE: &str = r#"{"version":1,"roles":{"abby-normal":{"profile":"x"}},"profiles":{"gemini-managed":{"runtime":"gemini","worker_args":["a"],"adapter_config":{"k":{"n":1}}}},"peer_uids":[1]}"#;
    const PROFILES_TWO: &str = r#"{"profiles":{"a":{"x":{}},"b":{}}}"#;

    #[test]
    fn profile_names_reads_only_the_top_level_profiles_keys() {
        assert_eq!(profile_names(PROFILES_ONE), vec!["gemini-managed".to_string()]);
        assert_eq!(profile_names(PROFILES_TWO), vec!["a".to_string(), "b".to_string()]);
        assert!(profile_names(r#"{"roles":{"profiles":{"nested":{}}}}"#).is_empty(), "a nested 'profiles' is not the top-level one");
        assert!(profile_names("not json").is_empty());
    }

    #[test]
    fn an_account_without_claude_gates_on_its_only_profile() {
        // #4866: Abby's cicd ran as chorus-abby-normal (no claude) and every gate failed.
        let got = choose_gate_profile(None, || false, || Some(PROFILES_ONE.to_string()));
        assert_eq!(got.as_deref(), Some("gemini-managed"));
    }

    #[test]
    fn negative_proof_claude_present_or_ambiguous_profiles_choose_nothing() {
        assert_eq!(choose_gate_profile(None, || true, || Some(PROFILES_ONE.to_string())), None, "an account with claude keeps the Claude path");
        assert_eq!(choose_gate_profile(None, || false, || Some(PROFILES_TWO.to_string())), None, "two profiles: never guess");
        assert_eq!(choose_gate_profile(None, || false, || None), None, "no profile file: still no model");
        assert_eq!(choose_gate_profile(Some("  "), || false, || None), None, "a blank override is no override");
    }

    #[test]
    fn an_explicit_gate_profile_still_wins() {
        let got = choose_gate_profile(Some("gate-chat"), || panic!("claude not consulted"), || panic!("file not read"));
        assert_eq!(got.as_deref(), Some("gate-chat"));
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

/// The card's title for the go row's subject ("" when the board cannot say).
fn go_row_title(home: &Path, card: u64) -> String {
    run(&script_path(home, "cards"), &["view", &card.to_string()]).map(|v| card_title(&v)).unwrap_or_default()
}
