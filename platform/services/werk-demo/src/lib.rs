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
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}
const LOCK_EX_NB: i32 = 0x02 | 0x04; // LOCK_EX | LOCK_NB
const LOCK_UN: i32 = 0x08;

pub type R<T> = Result<T, String>;

// --- pure helpers (unit-tested) ---

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

/// Given the werk-demo witness content + a card id, return the required gates
/// with NO demo.gate.result recorded for the card. Empty = the full gate gather
/// ran → a verdict may be recorded. card_id is matched comma-terminated so a
/// gate for #31 can't satisfy #3 (the #3116/#31160 collision class).
/// #3319 (Jeff's JX) — the announce is the ready-gate. The announce-bearing tail of
/// demo() (signal → test-surface → DEMO READY → peer feedback) may fire ONLY when this
/// returns true: every required gate is recorded for the card, OR the test suite is
/// driving the tail directly (skip_gate_check). False ⇒ stand by silently, no announce —
/// Jeff is never pulled into a demo before its gates exist.
pub fn announce_ready(witness: &str, card: u64, round: &str, skip_gate_check: bool) -> bool {
    skip_gate_check || gates_missing(witness, card, round).is_empty()
}

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

/// Resolve the round (HEAD short-12) from the CARD's werk, whichever role owns
/// it. "round-unknown" only when no role has a werk for the card.
pub fn round_for_card(werk_base: &str, card: u64) -> String {
    for r in ["wren", "silas", "kade"] {
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

/// A witness line matches the current round iff it carries this round's key.
/// Pre-#3365 records have NO round field and therefore match nothing — old
/// evidence ages out structurally the moment this lands.
fn line_in_round(line: &str, round: &str) -> bool {
    line.contains(&format!("\"round\":\"{}\"", round))
}

/// #3305 AC2/AC3 — the peers actually OWED a gather nudge this round: exactly
/// gathers_missing. A peer whose reply is on the witness for THIS round is
/// never re-nudged, however many times the demo re-presents. A new round
/// re-asks everyone by design (#3365 round discipline, not the re-fire bug).
pub fn nudge_targets(witness: &str, card: u64, role: &str, round: &str) -> Vec<&'static str> {
    gathers_missing(witness, card, role, round)
}

pub fn announce_ready_full(witness: &str, card: u64, role: &str, round: &str, skip: bool) -> bool {
    skip || (gates_missing(witness, card, round).is_empty() && gathers_missing(witness, card, role, round).is_empty())
}

/// The two peer roles owed a 4-question gather for this demo: everyone but the
/// demoer. (jeff is the prover, not a gather peer.)
pub fn gather_peers(role: &str) -> Vec<&'static str> {
    ["wren", "silas", "kade"].into_iter().filter(|r| *r != role).collect()
}

/// Peers with NO demo.gather.replied recorded for the card. Empty = both
/// peers' feedback is IN — the announce may fire. Mirrors gates_missing
/// (comma-terminated card match, same anti-collision rule). A gather reply is
/// recorded via `werk-demo gather <card> <peer> replied` when the peer's ACK
/// arrives — evidence on the witness, never a model claim.
pub fn gathers_missing(witness: &str, card: u64, role: &str, round: &str) -> Vec<&'static str> {
    let card_key = format!("\"card_id\":{},", card);
    gather_peers(role)
        .into_iter()
        .filter(|peer| {
            let peer_key = format!("\"peer\":\"{}\"", peer);
            !witness.lines().any(|l| {
                l.contains("\"event\":\"demo.gather.replied\"")
                    && l.contains(&card_key)
                    && l.contains(&peer_key)
                    && line_in_round(l, round)
            })
        })
        .collect()
}

pub fn gates_missing(witness: &str, card: u64, round: &str) -> Vec<&'static str> {
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
                    && line_in_round(l, round)
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

/// The product/domain feedback gather sent to each demoee (#3100 / #3116).
/// VERBATIM — pinned by `feedback_message_is_verbatim`. Neutral framing by
/// design: pointers + ask, no editorializing that biases the reply; sender +
/// ack-required up front; "read the card and the code" (recipient forms their
/// own read). NO "before /acp" pressure, NO "narrow/clean/delivered" pre-frame —
/// those inherit the builder's satisfaction to the reviewer. Q2 addresses the
/// recipient AS the user ("you and your domain"); Q4 is the Loom-discriminator
/// lens no peer can answer for. `\n` is escaped because the string is embedded
/// into a JSON body downstream.
///
/// Prior (#3100): 3 questions, Q2 = "how does it impact your users?".
/// Current (#3116): Q2 named to "you and your domain" (the demoee IS the user);
/// +Q4 Loom lens (the discriminator no peer can answer for). Approach: extract
/// to a pure fn so the wording is pinned by test and can't silently soften.
pub fn feedback_message(card: u64, from: &str) -> String {
    format!(
        "[feedback #{} — ACK REQUIRED]\\nFrom: {}\\nRead the card. Read the code. Then reply.\\n(1) How does this impact your products?\\n(2) How does this impact you and your domain?\\n(3) Am I over-building or under-planning?\\n(4) Does this strengthen Loom, or just please the room?\\nAck: substantive reply or blocked-on-X within 10 min.",
        card, from
    )
}

/// Send a feedback nudge to `other` via the chorus_nudge_message MCP path.
/// The team's canonical nudge surface — JSON-RPC tools/call POST'd to the
/// MCP server's HTTP endpoint. Body shape matches the MCP tool's NudgeInput.
/// Returns Err with curl's exit if the POST fails (status check via -f).
/// Used by signal() for the initial round and by demo() for re-nudge on
/// unacked peers (#3100 AC #2).
/// #3305 — pure wire shape for the gather nudge: JSON-RPC tools/call on
/// chorus_nudge_message. Unit-pinned (the e2e happy path now legitimately
/// sends zero nudges, so the shape can't be pinned there).
pub fn mcp_nudge_body(to: &str, message: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"chorus_nudge_message","arguments":{{"to":"{}","message":"{}"}}}}}}"#,
        to, message
    )
}

fn send_mcp_nudge(from: &str, other: &str, card: u64, trace: &str) -> R<()> {
    let mcp_url = std::env::var("CHORUS_MCP_URL")
        .unwrap_or_else(|_| "http://localhost:3341/mcp".to_string());
    let body = mcp_nudge_body(other, &feedback_message(card, from));
    run("curl", &[
        "-s", "-f", "-X", "POST",
        &mcp_url,
        "-H", "Content-Type: application/json",
        // chorus-mcp requires BOTH content types in Accept (Silas #3092 trap).
        "-H", "Accept: application/json, text/event-stream",
        "-H", &format!("X-Chorus-Role: {}", from),
        "-H", &format!("X-Chorus-Trace-Id: {}", trace),
        "-d", &body,
    ]).map(|_| ())
}

/// Step 5: signal — cards demo + spine event + Bridge post + feedback nudges.
/// All four are best-effort (the act has already gated; signal is the announcement,
/// not a gate). Bridge + nudges are HTTP POSTs to localhost services (zero-dep:
/// curl as a subprocess, mirroring the verb-contract).
fn signal(card: u64, role: &str, home: &Path, trace: &str, owed: &[&str]) -> Vec<String> {
    let card_s = card.to_string();
    // board demo signal
    let _ = run(&script_path(home, "cards"), &["demo", &card_s]);
    emit_spine(home, "card.demo.started", role, card, trace);

    // Bridge post (localhost:3470 — Jeff's center panel)
    let bridge_body = format!(
        r#"{{"from":"{}","text":"[demo] #{} — werk-demo: presenting the running werk variant for review"}}"#,
        role, card
    );
    let _ = run(
        "curl",
        &[
            "-s", "-X", "POST",
            "http://localhost:3470/api/message",
            "-H", "Content-Type: application/json",
            "-d", &bridge_body,
        ],
    );

    // Feedback nudges go through the chorus_nudge_message MCP tool — the team's
    // canonical nudge surface, via send_mcp_nudge() (shared with the re-nudge
    // path in demo() for AC #2).
    //
    // #3352 — a failed gather send is LOUD, never silent. The old behavior wrote
    // demo.nudge.failed to a log nobody reads and walked on as if the team had
    // been asked — the "secret back door" Jeff named: the ceremony's sends fail
    // mid-pipeline under load and nobody learns until he asks "did u nudge the
    // team". Failed peers are returned so demo() puts them ON the surface.
    let mut send_failed: Vec<String> = Vec::new();
    // #3305 — only the peers still OWED this round (nudge_targets). Previously
    // this loop hit BOTH peers unconditionally on every re-present: 8 sightings
    // in one day (5x silas, refire storms on wren + kade), each one an
    // ACK-REQUIRED on an already-answered gather — the 2-touch contract broken
    // by the ceremony itself.
    for other in owed.iter() {
        if let Err(e) = send_mcp_nudge(role, other, card, trace) {
            jsonl(home, role, card, trace, "demo.nudge.failed",
                  &format!(",\"to\":\"{}\",\"reason\":\"{}\"", other, e.replace('"', "'")));
            send_failed.push(other.to_string());
        }
    }
    send_failed
}

// --- the demo act ---

/// Testable core — all inputs explicit. #3116: the proving ceremony only —
/// validate → present → feedback gather → review window → verdict. The act
/// (build/deploy/env-up) is done by the PRIOR verbs in the flat sequence; the
/// gates run as subagents in the /demo skill layer. demo records demo.verdict;
/// werk-accept gates finalize on it.
pub fn demo(card: u64, role: &str, home: &Path) -> R<DemoOutcome> {
    let trace = env::var("CHORUS_TRACE_ID").unwrap_or_else(|_| trace_id());
    jsonl(home, role, card, &trace, "demo.started", "");

    let card_s = card.to_string();

    // Step 1: validate — exists + WIP/Now.
    let cj = run(&script_path(home, "cards"), &["view", &card_s, "--json"])
        .map_err(|e| format!("validate: cannot read card #{}: {}", card, e))?;
    let status = json_str_field(&cj, "status").unwrap_or_default();
    if status != "WIP" && status != "Now" {
        jsonl(home, role, card, &trace, "demo.refused", ",\"reason\":\"wrong-status\"");
        emit_spine_reason(home, "demo.refused", role, card, &trace, "wrong-status");
        return Err(format!("#{} is {} — must be WIP/Now to demo", card, status));
    }

    // Step 1.5: AC pre-flight — all AC checked (uses the human view for checkboxes).
    let cv = run(&script_path(home, "cards"), &["view", &card_s])?;
    let (checked, total) = ac_counts(&cv);
    if total == 0 {
        jsonl(home, role, card, &trace, "demo.refused", ",\"reason\":\"no-ac\"");
        emit_spine_reason(home, "demo.refused", role, card, &trace, "no-ac");
        return Err(format!("#{} has no acceptance criteria", card));
    }
    // #3263 — AC completeness is INFORMATIONAL, not a refuse. The demo DEMONSTRATES
    // the AC and Jeff's go is the verification; pre-ticked boxes are not a precondition
    // (that's the "self-attest then show" model this card replaces — you can't honestly
    // tick a demonstrable AC before the demo that proves it). The count rides the
    // decision surface so Jeff sees AC X/Y before deciding — informed, not gated.
    jsonl(home, role, card, &trace, "demo.ac_status",
          &format!(",\"checked\":{},\"total\":{}", checked, total));
    // Post the SINGLE gate-evidence comment + trace file (#2910 / #2897). Without
    // these, /acp will refuse "no demo evidence" via done-gate.sh / accept_gate.
    post_preflight_evidence(home, card, role, checked, total, &trace)?;
    write_trace_file(card, &trace);
    jsonl(home, role, card, &trace, "demo.preflight.passed", &format!(",\"ac\":\"{}/{}\"", checked, total));

    // INVARIANT GATE EXECUTION (#3237/#3284, now #3443). Refuse to PRESENT unless
    // all 5 gates left a demo.gate.result in the witness. This blocks presenting
    // UN-GATED; it never blocks Jeff's go (#3263/DEC-048 sovereign-go intact —
    // gates inform, never veto). #3443: the binary RUNS the gates itself (headless
    // claude -p, see run_gates below) just before this check, so the refusal now
    // only triggers where gates genuinely couldn't run. Refusing here, before any
    // announce, is what makes a gate-less demo fail LOUD instead of silently
    // presenting "(none run)". Skippable only in the unit/e2e suite.
    //
    // #3318/#3443 — the act/CI degrade. #3318 SKIPPED enforcement under act/CI
    // because "no agent to run gates" there. #3443 retires that excuse: where the
    // claude binary resolves (Jeff's box, local act) the binary self-gates and we
    // ENFORCE. The only remaining skip is hosted CI with no claude (gates_ran=false
    // AND in_act) — degrade, don't break the build. Detected via ACT/GITHUB_ACTIONS.
    let in_act = std::env::var("ACT").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok();
    let skip_gate_check =
        std::env::var("CHORUS_DEMO_SKIP_GATE_CHECK").map(|v| v == "1").unwrap_or(false);
    // #3365 — the round this demo proves: the werk's HEAD sha. All step
    // evidence (gates, gathers, announce) must carry it; prior rounds' records
    // never satisfy this one.
    let round = current_round(role, card);
    // #3443 AC1 — RUN the gates ourselves before enforcing their presence. Where
    // claude is available (Jeff's box, local act) the binary self-gates: one
    // headless `claude -p` per absent gate, recorded via record_gate. This kills
    // the demoer-agent dependency (#3116) AND the act/CI standby (#3318) — gates
    // now run in the headless path too. Skippable only in the unit/e2e suite,
    // which seeds its own gate results.
    let skip_gate_run = std::env::var("CHORUS_DEMO_SKIP_GATE_RUN").map(|v| v == "1").unwrap_or(false);
    let gates_ran = !skip_gate_run && claude_available();
    if gates_ran {
        run_gates(home, role, card, &round, &trace);
    }
    // Enforce gate presence whenever we ran them, or whenever we're in the
    // interactive (non-act) path. The only path that still skips enforcement is
    // hosted CI with no claude binary (gates_ran=false AND in_act) — degrade, do
    // not break the build (#3284 lesson).
    if !skip_gate_check && (gates_ran || !in_act) {
        let witness = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
        let absent = gates_missing(&witness, card, &round);
        if !absent.is_empty() {
            jsonl(home, role, card, &trace, "demo.refused",
                  &format!(",\"reason\":\"gates-missing\",\"missing\":\"{}\"", absent.join(",")));
            emit_spine_reason(home, "demo.refused", role, card, &trace, "gates-missing");
            return Ok(DemoOutcome {
                message: format!(
                    "demo #{} REFUSED to present — gates not run: [{}]. All 5 gates \
                     (product, code, quality, arch, ops) must record a demo.gate.result before \
                     the demo presents (invariant execution, #3284). Run them via the /demo \
                     skill's gate subagents, then re-run. Jeff's GO stays sovereign — this blocks \
                     presenting UN-GATED, never your go.",
                    card, absent.join(", ")
                ),
                exit: 1,
            });
        }
    }

    // #3263 — THE DEMO RUNS THE TESTS ITSELF and records the result. This ends the
    // "did the tests actually run?" argument: the evidence is the artifact (a recorded
    // demo.test_result + spine event), not a human claim. It does NOT gate — a red or
    // un-run result is SHOWN on the decision surface; Jeff's go stays sovereign.
    // Skippable only in the unit/e2e suite (which seeds its own result, no real werk).
    if !std::env::var("CHORUS_DEMO_SKIP_TEST_RUN").map(|v| v == "1").unwrap_or(false) {
        let werk_base = env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
            format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
        });
        let werk = format!("{}/{}-{}", werk_base, role, card);
        let res = if run_card_tests(&werk) { "pass" } else { "fail" };
        record_test_result(home, role, card, res);
        jsonl(home, role, card, &trace, "demo.test_ran", &format!(",\"result\":\"{}\"", res));
    }

    // #3443 — the GATE step is RUN BY THIS BINARY (run_gates above), not delegated
    // to the /demo skill's subagents. The owning role still REVIEWS its recorded
    // result async, but execution is the binary's. The old go-run-your-gate nudge
    // relay + the in-binary gate-chain wait stay retired (the agents-grading-agents
    // relay was the waste, not the gates). Smoke folds into the machine prover.
    emit_spine(home, "demo.gates.recorded", role, card, &trace);

    // #3319 (Jeff's JX, 2026-06-10): THE ANNOUNCE IS THE READY-GATE. The
    // announce-bearing tail below (signal → test-surface → DEMO READY → peer
    // feedback) fires ONLY when prework is sealed — every required gate recorded.
    // The interactive demoer already REFUSED above (~line 590) if gates are
    // missing; the only way to reach here un-gated is the headless act prework
    // path, which skips that refusal because it has no LLM to run gates. In that
    // path we STAND BY: the variant is up, gates are pending the interactive
    // demoer, and we emit NO announce — no Bridge post, no peer nudges, no
    // "DEMO READY". Jeff is never dragged into a demo before it can run. The
    // demoer runs the 5 gates as prework (/demo Step 1), then re-invokes; on that
    // pass gates are present and the announce fires for real.
    // (skip_gate_check = the unit/e2e suite, which drives the full tail directly.)
    {
        let witness_pre = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
        // #3352 — the full invariant: gates recorded AND both peer gathers REPLIED.
        // Jeff's spec: "announce happens b4 role feedback so go precedes that" — the
        // announce may not exist until the team's feedback is IN. Typed standby names
        // exactly what's missing so the demoer knows the next prework step.
        if !announce_ready_full(&witness_pre, card, role, &round, skip_gate_check) {
            let gates_absent = gates_missing(&witness_pre, card, &round);
            let gathers_absent = gathers_missing(&witness_pre, card, role, &round);
            let reason = if !gates_absent.is_empty() { "gates-pending" } else { "gathers-pending" };
            emit_spine(home, "demo.prework.standby", role, card, &trace);
            jsonl(home, role, card, &trace, "demo.prework.standby",
                  &format!(",\"reason\":\"{}\",\"gates_missing\":\"{}\",\"gathers_missing\":\"{}\"",
                           reason, gates_absent.join(","), gathers_absent.join(",")));
            return Ok(DemoOutcome {
                message: format!(
                    "demo #{} — prework standby ({}, round {}). Variant is up; NO announce fired (the \
                     announce is the ready-gate: gates recorded AND peer gathers replied, \
                     #3319+#3352, all keyed to THIS round). Missing gates: [{}]. Missing gather replies: [{}]. The \
                     demoer completes prework (run gates via `werk-demo gate`, send the \
                     4-question gathers, record replies via `werk-demo gather <card> <peer> \
                     replied`), then re-invokes. Jeff is never asked for a go that precedes \
                     the team's feedback.",
                    card, reason, round, gates_absent.join(","), gathers_absent.join(",")
                ),
                exit: 0,
            });
        }
    }

    // Step 5: signal — board demo + spine event + Bridge + feedback nudges (best-effort,
    // the act has already gated; this announces). Step 4 stakes-brief is human-driven
    // content; demo-v2 records it in spine events, not as a separate gate.
    // #3305 — fresh witness read at the send site: only peers still owed THIS
    // round get the 4-question nudge; an acked peer is never re-fired however
    // many times the demo re-presents.
    let witness_now = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
    let owed = nudge_targets(&witness_now, card, role, &round);
    let gather_send_failed = signal(card, role, home, &trace, &owed);
    if !gather_send_failed.is_empty() {
        // #3352 — surface the send failure in the returned message itself: the
        // demoer (and Jeff) see it in-window and send by hand; the announce
        // stays held anyway (gathers_missing requires replies).
        jsonl(home, role, card, &trace, "demo.gather.send_failed",
              &format!(",\"peers\":\"{}\"", gather_send_failed.join(",")));
        return Ok(DemoOutcome {
            message: format!(
                "demo #{} — GATHER SEND FAILED to [{}]. The feedback nudges did NOT \
                 reach those peers (MCP send error mid-pipeline — see demo.nudge.failed \
                 in the witness). Send the 4 questions by hand (werk-demo gather-text {}) \
                 and record replies (werk-demo gather {} <peer> replied), then re-invoke. \
                 No announce until the team is actually asked.",
                card, gather_send_failed.join(","), card, card
            ),
            exit: 0,
        });
    }
    jsonl(home, role, card, &trace, "demo.signal.completed", "");

    // #3116 — the ACT is OUT of demo. build → deploy → env-up are the PRIOR
    // atomic verbs in the flat sequence (werk-mcp.sh steps 3-4); they stand up
    // the role's werk variant. Demo only POINTS at that already-running instance
    // — it never builds or deploys. (Boundary confirmed with Kade, #3211/#3222.)

    // #3100 — announce the TEST SURFACE before the test window opens. Names
    // service ports + CLI-verb binary paths so the demo-er + team + Jeff know
    // exactly what new code is running and where to hit it. Without this, the
    // pause is a "comment window" with no surface to comment on; with it, the
    // pause becomes a real test window. Silas paired the framing on #3101.
    let api_port = match role { "silas" => 3343, "kade" => 3344, "wren" => 3345, _ => 3340 };
    let mcp_port = match role { "silas" => 3351, "kade" => 3352, "wren" => 3353, _ => 3341 };
    let test_surface_body = format!(
        r#"{{"from":"{}","text":"━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n🧪 [TEST SURFACE READY] — card #{}\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\nService variants: chorus-api http://localhost:{}/, chorus-mcp http://localhost:{}/mcp\nCLI verbs (if changed): resolve via {}'s session PATH (role-slot-first per #3101)\nWhat's new: read the card, the diff, then exercise the new code against the surfaces above.\nThis is the test window — substantive trial before /acp."}}"#,
        role, card, api_port, mcp_port, role
    );
    if let Err(e) = run("curl", &[
        "-s", "-f", "-X", "POST",
        "http://localhost:3470/api/message",
        "-H", "Content-Type: application/json",
        "-d", &test_surface_body,
    ]) {
        jsonl(home, role, card, &trace, "demo.bridge.failed",
              &format!(",\"reason\":\"test_surface:{}\"", e.replace('"', "'")));
    }
    emit_spine(home, "demo.test_surface.ready", role, card, &trace);
    jsonl(home, role, card, &trace, "demo.test_surface.ready", "");

    // #3109 Bug 1 fix: `demo.show.completed` previously fired here, right after
    // deploy and BEFORE the peer-engagement check below. Since /acp's accept_gate
    // reads `demo.show.completed`, that order let /acp admit a card before peers
    // had actually exercised the variant — the contamination Silas hit on #3109
    // today. The emission is now moved to AFTER the peer-engagement loop and
    // gated on no escalations; if any peer failed to exercise, the demo emits
    // `demo.show.refused` instead and /acp refuses.

    // #3100 AC#4 — visible announce. Owner/head-of-product (Jeff) gets a
    // framed shape he cannot miss: [DEMO READY FOR JEFF] banner prefix, card
    // id, variant URL, explicit react prompt. Not a scrollable Bridge line.
    // Per-role werk-api ports per #3092 (silas=3343, kade=3344, wren=3345);
    // canonical 3340 fallback for unknown role.
    let variant_port = match role {
        "silas" => 3343,
        "kade"  => 3344,
        "wren"  => 3345,
        _ => 3340,
    };
    let variant_url = format!("http://localhost:{}/api/chorus/health", variant_port);
    let pause_body = format!(
        r#"{{"from":"{}","text":"━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n🎬 [DEMO READY FOR JEFF] — card #{}\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\nVariant up: {}\nAwaiting your eyes (or a machine verdict).\n→ React with questions, check the variant, or /acp when satisfied."}}"#,
        role, card, variant_url
    );
    // -f + exit-check so the silent-success class can't recur on this surface
    // (Kade's debt-note catch — AC2 spirit leaks beyond signal()).
    if let Err(e) = run(
        "curl",
        &[
            "-s", "-f", "-X", "POST",
            "http://localhost:3470/api/message",
            "-H", "Content-Type: application/json",
            "-d", &pause_body,
        ],
    ) {
        jsonl(home, role, card, &trace, "demo.bridge.failed",
              &format!(",\"reason\":\"{}\"", e.replace('"', "'")));
    }
    jsonl(home, role, card, &trace, "demo.ready_for_review", "");

    // #3263 — THE MACHINE SHOWS, IT DOES NOT GATE. Jeff's go is sovereign (DEC-048):
    // the demo presents the truth so the decision is never blind, but it NEVER refuses
    // his go over test / variant / AC status — "if I say go it does not matter if tests
    // ran." Below we only COMPUTE honest status for the decision surface; no refusals.
    let witness = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();

    // running: is the variant actually reachable right now? Informational only.
    let skip_variant_check =
        std::env::var("CHORUS_DEMO_SKIP_VARIANT_CHECK").map(|v| v == "1").unwrap_or(false);
    let variant_status = if skip_variant_check {
        format!("running → {}", variant_url)
    } else if run("curl", &["-s", "-f", "-o", "/dev/null", "--max-time", "5", &variant_url]).is_ok() {
        format!("running → {} (reachable)", variant_url)
    } else {
        format!("running → {} (NOT reachable)", variant_url)
    };

    // tested: did the demo's test run record a result? (informational on the surface)
    let test_res = test_result_recorded(&witness, card);
    let test_summary = match &test_res {
        Some(r) => format!("tests: {}", r),
        None => "tests: NOT run".to_string(),
    };

    // gates: #3284 (AC7) — each gate's VERDICT (✓/✗/-), not just which ran, so the
    // decision surface carries the feedback (the #3251 residual).
    let gate_summary = render_gate_summary(&witness, card);

    // #3284 (AC1-4) — the execution-state cockpit, from the card's real events.
    let events = fetch_card_events(card);
    let cockpit = render_execution_state(&events, test_res.as_deref());
    let gate_feedback = render_gate_feedback(&witness, card);

    // #3284 — THE ANNOUNCE, in Jeff's 5-step order: gates required (gate_summary) ·
    // feedback required (gate_feedback) · announced here · then he asks questions /
    // asks to test · then go / no. Built ONCE as plain text and RETURNED as this
    // verb's message — because in auto/focus mode Jeff sees only the agent's
    // end-of-turn reply, never a Bridge post; the agent pastes this verbatim.
    let feedback_block = if gate_feedback.is_empty() {
        String::new()
    } else {
        format!("\n{}", gate_feedback)
    };
    let announce = format!(
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n🎬 #{} — DEMO · ready for your review   AC {}/{}\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n{}\n{}{}\nvariant: {}\n{}\n──────────────────────────────────\nAsk me anything about this, or tell me to TEST the change against the variant.\nThen your call → go / no / more.",
        card, checked, total,
        cockpit,
        gate_summary,
        feedback_block,
        variant_status,
        test_summary,
    );
    // Also POST to Bridge (history + any non-focus surface). Escaped for one-line JSON.
    let surface_body = format!(
        r#"{{"from":"{}","text":"{}"}}"#,
        role,
        announce.replace('\\', " ").replace('"', "'").replace('\n', "\\n")
    );
    let _ = run("curl", &["-s", "-f", "-X", "POST", "http://localhost:3470/api/message",
                          "-H", "Content-Type: application/json", "-d", &surface_body]);
    jsonl(home, role, card, &trace, "demo.decision_surface",
          &format!(",\"ac\":\"{}/{}\"", checked, total));

    // #3279 — PRESENT-AND-EXIT. The demo no longer BLOCKS for the decision. Blocking
    // here held the synchronous MCP call open for the entire human wait, and a held
    // call drops on long waits (the client↔chorus-mcp transport self-sever, #3277) —
    // it could not survive a 10-12h walk-away. The earlier "fix" (detach the run)
    // then lost Jeff's in-session visibility entirely (he saw nothing, was asked to
    // approve blind). Both are wrong. So the demo PRESENTS (variant up + decision
    // surface, posted above) and EXITS cleanly — Half A of the pipeline ends here.
    // NOTHING is held: there is no wait to drop and no detached process to leak.
    // Jeff decides whenever — minutes, or hours, or after a reboot — and his GO runs
    // Half B (werk.yml's go-gated `land` job: merge → sync → deploy → accept). The "wait"
    // costs nothing because it is a stopped pipeline, not a held connection.
    jsonl(home, role, card, &trace, "demo.presented",
          &format!(",\"ac\":\"{}/{}\",\"round\":\"{}\",\"variant\":\"{}\"", checked, total, round, variant_url));
    emit_spine(home, "demo.presented", role, card, &trace);
    // #3284 — RETURN the announce as the verb's message. In auto/focus mode the agent
    // pastes this verbatim into its end-of-turn reply — the ONLY surface Jeff sees.
    // The Bridge post above is the non-focus mirror. (Your GO runs Half B / werk-land.)
    jsonl(home, role, card, &trace, "demo.completed", ",\"phase\":\"presented\"");
    Ok(DemoOutcome { message: announce, exit: 0 })
}

/// #3237 — record one gate's result into the witness so the verdict step can
/// verify the gate gather ran. Called by the /demo skill's gate subagents via
/// `werk-demo gate <card> <gate> <result>`. The witness is the same
/// ops/logs/werk-demo.jsonl the verdict + werk-accept read — one evidence file.
fn record_gate(home: &Path, role: &str, card: u64, gate: &str, result: &str, findings: &str, round: &str) {
    let trace = env::var("CHORUS_TRACE_ID").unwrap_or_else(|_| trace_id());
    // #3284 — feedback is REQUIRED: a gate carries WHAT it found, not just pass/fail,
    // so the announce shows real feedback. Sanitized for the one-line JSONL witness.
    let f = findings.replace('\\', " ").replace('"', "'").replace('\n', " ");
    jsonl(home, role, card, &trace, "demo.gate.result",
          &format!(",\"gate\":\"{}\",\"result\":\"{}\",\"round\":\"{}\",\"findings\":\"{}\"", gate, result, round, f));
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

/// Is the `claude` CLI resolvable + runnable? Gates can only self-run where a
/// real claude binary exists (Jeff's machine, local `act`). On hosted CI (no
/// auth, no binary) it is absent — we degrade rather than break the build
/// (#3284 lesson). Best-effort: a non-zero/failed `--version` means "no".
fn claude_available() -> bool {
    Command::new("claude").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
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
fn run_one_gate(home: &Path, role: &str, card: u64, gate: &str, round: &str) {
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
    let diff = Command::new("git")
        .arg("-C").arg(&werk)
        .args(["diff", "origin/main...HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    // Cap the diff so a huge change can't blow the context budget.
    let diff_capped: String = diff.chars().take(60_000).collect();
    let ctx = format!(
        "CARD #{}\n{}\n\n=== BRANCH DIFF (origin/main...HEAD) ===\n{}",
        card, card_view, diff_capped
    );

    let model = env::var("CHORUS_GATE_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".to_string());
    let child = Command::new("claude")
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
    }
    let out = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => {
            record_gate(home, role, card, gate, "error", &format!("claude wait failed: {}", e), round);
            return;
        }
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        record_gate(home, role, card, gate, "error",
                    &format!("claude exit {:?}: {}", out.status.code(), err.chars().take(160).collect::<String>()),
                    round);
        return;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let (result, findings) = parse_gate_verdict(&stdout);
    record_gate(home, role, card, gate, &result, &findings, round);
}

/// AC1 (#3443) — run every required gate that has NO result for THIS round,
/// recording each itself. Already-recorded gates (e.g. a re-invoke after a drop)
/// are not re-run. The demo no longer depends on a demoer agent to spawn gates.
fn run_gates(home: &Path, role: &str, card: u64, round: &str, trace: &str) {
    let witness = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
    let absent = gates_missing(&witness, card, round);
    if absent.is_empty() {
        return;
    }
    emit_spine(home, "demo.gates.running", role, card, trace);
    for gate in absent {
        run_one_gate(home, role, card, gate, round);
    }
}

/// #3263 — record the pipeline's test outcome into the witness so the informed-go
/// check can require it (and Jeff sees it before the go). Called by werk.yml's test
/// step via `werk-demo test-result <card> pass|fail`. pass AND fail are recorded —
/// a red test is VISIBLE, not hidden; blocking-on-red is #3190's promotion, but the
/// result existing at all is what makes "I didn't/can't test" unreachable for a go.
fn record_test_result(home: &Path, role: &str, card: u64, result: &str) {
    let trace = env::var("CHORUS_TRACE_ID").unwrap_or_else(|_| trace_id());
    jsonl(home, role, card, &trace, "demo.test_result",
          &format!(",\"result\":\"{}\"", result));
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

/// CLI shim: parse args/env only, then call the testable core (blueprint pattern).
/// Two forms: `werk-demo <card-id>` (the ceremony) and `werk-demo gate <card>
/// <gate> <result>` (a gate subagent recording its result, #3237).
pub fn run_demo() -> R<DemoOutcome> {
    let role = env::var("DEPLOY_ROLE").unwrap_or_default();
    if role.trim().is_empty() {
        return Err("DEPLOY_ROLE unset — cannot demo without a role".to_string());
    }
    let home = env::var("CHORUS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&env::var("HOME").unwrap_or_default()).join("CascadeProjects/chorus"));

    let mut args = env::args().skip(1);
    let first = args
        .next()
        .ok_or("usage: werk-demo <card-id> | werk-demo gate <card> <gate> <result>")?;

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

    if first == "gather-text" {
        // #3352 — the 4-question gather is VERBATIM from the verb, not improvised
        // per-demoer. The demoer (or orchestrator) sends exactly this text; wording
        // drift across roles/models is impossible.
        let card: u64 = args
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or("usage: werk-demo gather-text <card>")?;
        return Ok(DemoOutcome {
            message: format!(
                "[feedback #{c} — ACK REQUIRED] From: {r}. Read the card. Read the diff (werk {r}-{c}). \
                 (1) How does this impact your products? (2) How does this impact you and your domain? \
                 (3) Am I over-building or under-planning? (4) Does this strengthen the system, or just \
                 please the room? Ack: substantive reply or blocked-on-X within 10 min. Demo HOLDS for \
                 your reply — record it with: werk-demo gather {c} <your-role> replied",
                c = card, r = role.trim()
            ),
            exit: 0,
        });
    }

    if first == "gather" {
        // #3352 — record gather lifecycle evidence on the witness: sent when the
        // 4-question nudge goes out, replied when the peer's ACK arrives. The
        // announce requires `replied` from every peer (gathers_missing); a model
        // cannot claim feedback that isn't witnessed.
        let card: u64 = args
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or("usage: werk-demo gather <card> <peer> <sent|replied> [note]")?;
        let peer = args.next().ok_or("usage: werk-demo gather <card> <peer> <sent|replied> [note]")?;
        if !["wren", "silas", "kade"].contains(&peer.as_str()) {
            return Err(format!("unknown peer '{}' — one of wren|silas|kade", peer));
        }
        let phase = args.next().unwrap_or_else(|| "replied".to_string());
        if !["sent", "replied"].contains(&phase.as_str()) {
            return Err(format!("unknown gather phase '{}' — sent|replied", phase));
        }
        let note = args.collect::<Vec<_>>().join(" ").replace('\\', " ").replace('"', "'").replace('\n', " ");
        let trace = env::var("CHORUS_TRACE_ID").unwrap_or_else(|_| trace_id());
        let event = format!("demo.gather.{}", phase);
        let round = current_round(role.trim(), card);
        jsonl(&home, role.trim(), card, &trace, &event,
              &format!(",\"peer\":\"{}\",\"round\":\"{}\",\"note\":\"{}\"", peer, round, note));
        emit_spine(&home, &event, role.trim(), card, &trace);
        return Ok(DemoOutcome {
            message: format!("gather {} recorded for #{}: peer={}", phase, card, peer),
            exit: 0,
        });
    }

    if first == "test-result" {
        let card: u64 = args
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or("usage: werk-demo test-result <card> <pass|fail>")?;
        let result = args.next().unwrap_or_else(|| "pass".to_string());
        record_test_result(&home, role.trim(), card, &result);
        return Ok(DemoOutcome {
            message: format!("test result recorded for #{}: {}", card, result),
            exit: 0,
        });
    }

    let card: u64 = first.parse().map_err(|_| "usage: werk-demo <card-id>")?;
    demo(card, role.trim(), &home)
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

    #[test]
    fn gate_system_prompt_embeds_skill_and_output_contract() {
        let p = gate_system_prompt("# Gate Product\nCheck the AC.");
        assert!(p.contains("Gate Product"));
        assert!(p.contains("\"result\":\"pass\""));
        assert!(p.contains("ONLY a single JSON object"));
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

    #[test]
    fn feedback_message_is_verbatim() {
        // Pinned byte-for-byte (#3116). Q2 = "you and your domain" (demoee is the
        // user); Q4 = the Loom discriminator lens. Changing this is a deliberate
        // product decision, not a refactor — this test is the guard that makes it so.
        let expected = "[feedback #3116 — ACK REQUIRED]\\nFrom: wren\\nRead the card. Read the code. Then reply.\\n(1) How does this impact your products?\\n(2) How does this impact you and your domain?\\n(3) Am I over-building or under-planning?\\n(4) Does this strengthen Loom, or just please the room?\\nAck: substantive reply or blocked-on-X within 10 min.";
        assert_eq!(feedback_message(3116, "wren"), expected);
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
        assert!(gates_missing(&w, 3237, "r1").is_empty(), "all 5 gates present → none missing");
    }

    #[test]
    fn gates_missing_lists_absent_gates() {
        let w = format!("{}\n{}", gate_line(3237, "product"), gate_line(3237, "code"));
        assert_eq!(gates_missing(&w, 3237, "r1"), vec!["quality", "arch", "ops"]);
    }

    #[test]
    fn gates_missing_all_when_witness_empty() {
        assert_eq!(gates_missing("", 3237, "r1").len(), 5);
    }

    // #3319 — the announce is the ready-gate: no announce fires until gates are recorded.
    #[test]
    fn announce_blocked_when_any_gate_missing() {
        let w = format!("{}\n{}", gate_line(3319, "product"), gate_line(3319, "code"));
        assert!(!announce_ready(&w, 3319, "r1", false), "missing gates ⇒ stand by, no announce");
    }

    #[test]
    fn announce_allowed_when_all_five_recorded() {
        let w = REQUIRED_GATES.iter().map(|g| gate_line(3319, g)).collect::<Vec<_>>().join("\n");
        assert!(announce_ready(&w, 3319, "r1", false), "all 5 gates ⇒ announce may fire");
    }

    #[test]
    fn announce_blocked_when_witness_empty() {
        assert!(!announce_ready("", 3319, "r1", false), "no gates at all ⇒ never announce");
    }

    // #3324 AUDIT — announce_skip_drives_tail_for_test_suite deleted: with skip=true
    // announce_ready is `true || …` — the assert could not fail (passes-by-definition).

    // #3352 — the full invariant: gates AND gathers-replied before any announce.
    fn gather_line(card: u64, peer: &str) -> String {
        format!(
            r#"{{"ts":1,"event":"demo.gather.replied","role":"wren","card_id":{},"trace_id":"t","peer":"{}","round":"r1","note":"ack"}}"#,
            card, peer
        )
    }

    #[test]
    fn gather_peers_excludes_the_demoer() {
        assert_eq!(gather_peers("wren"), vec!["silas", "kade"]);
        assert_eq!(gather_peers("kade"), vec!["wren", "silas"]);
    }

    #[test]
    fn announce_full_blocked_when_gates_present_but_gathers_missing() {
        // The 2026-06-11 class: all 5 gates recorded, NO peer feedback — the old
        // announce fired here and Jeff's go preceded the team's input. Now: standby.
        let w = REQUIRED_GATES.iter().map(|g| gate_line(3352, g)).collect::<Vec<_>>().join("\n");
        assert!(!announce_ready_full(&w, 3352, "wren", "r1", false), "gates without gathers ⇒ no announce");
        assert_eq!(gathers_missing(&w, 3352, "wren", "r1"), vec!["silas", "kade"]);
    }

    #[test]
    fn announce_full_blocked_when_only_one_peer_replied() {
        let mut lines: Vec<String> = REQUIRED_GATES.iter().map(|g| gate_line(3352, g)).collect();
        lines.push(gather_line(3352, "kade"));
        let w = lines.join("\n");
        assert!(!announce_ready_full(&w, 3352, "wren", "r1", false), "one peer's reply is not the team's feedback");
        assert_eq!(gathers_missing(&w, 3352, "wren", "r1"), vec!["silas"]);
    }

    #[test]
    fn announce_full_fires_when_gates_and_both_gathers_replied() {
        let mut lines: Vec<String> = REQUIRED_GATES.iter().map(|g| gate_line(3352, g)).collect();
        lines.push(gather_line(3352, "silas"));
        lines.push(gather_line(3352, "kade"));
        let w = lines.join("\n");
        assert!(announce_ready_full(&w, 3352, "wren", "r1", false), "gates + both replies ⇒ announce");
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
        assert!(!announce_ready_full(&w, 3352, "wren", "r1", false), "sent != replied");
    }

    // #3365 — THE core proof: a prior round's records never satisfy this round.
    #[test]
    fn round2_with_only_round1_records_refuses_everything() {
        let mut lines: Vec<String> = REQUIRED_GATES.iter().map(|g| gate_line(3365, g)).collect();
        lines.push(gather_line(3365, "silas"));
        lines.push(gather_line(3365, "kade"));
        let w = lines.join("\n"); // a fully-passed ROUND r1
        assert!(announce_ready_full(&w, 3365, "wren", "r1", false), "r1 evidence satisfies r1");
        assert!(!announce_ready_full(&w, 3365, "wren", "r2", false), "r1 evidence must NOT satisfy r2");
        assert_eq!(gates_missing(&w, 3365, "r2").len(), 5, "all gates fresh per round");
        assert_eq!(gathers_missing(&w, 3365, "wren", "r2").len(), 2, "all feedback fresh per round");
    }

    #[test]
    fn pre_3365_records_without_round_field_match_nothing() {
        // Old evidence (no round field) ages out structurally on landing.
        let w = r#"{"ts":1,"event":"demo.gate.result","role":"wren","card_id":3365,"trace_id":"t","gate":"product","result":"pass"}"#;
        assert_eq!(gates_missing(w, 3365, "r1").len(), 5);
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
    fn mcp_nudge_body_pins_tools_call_shape() {
        let b = mcp_nudge_body("silas", "msg");
        assert!(b.contains("\"method\":\"tools/call\""));
        assert!(b.contains("\"name\":\"chorus_nudge_message\""));
        assert!(b.contains("\"to\":\"silas\""));
    }

    #[test]
    fn nudge_targets_skips_replied_peers_same_round() {
        // wren already replied this round → only silas is owed a nudge (AC2).
        let w = r#"{"ts":1,"event":"demo.gather.replied","role":"kade","card_id":3305,"trace_id":"t","peer":"wren","round":"r1","note":""}"#;
        assert_eq!(nudge_targets(w, 3305, "kade", "r1"), vec!["silas"]);
    }

    #[test]
    fn nudge_targets_empty_when_all_replied_no_refire_on_represent() {
        // both peers replied this round → re-presenting fires ZERO nudges (AC3:
        // re-running a demo does not reset acked peers; sightings #1-#8, 2026-06-12).
        let w = format!(
            "{}
{}",
            r#"{"ts":1,"event":"demo.gather.replied","role":"kade","card_id":3305,"trace_id":"t","peer":"wren","round":"r1","note":""}"#,
            r#"{"ts":2,"event":"demo.gather.replied","role":"silas","card_id":3305,"trace_id":"t","peer":"silas","round":"r1","note":""}"#
        );
        assert!(nudge_targets(&w, 3305, "kade", "r1").is_empty(), "no re-fire to acked peers");
    }

    #[test]
    fn nudge_targets_fresh_round_renudges_per_3365() {
        // a NEW round legitimately re-asks: #3365 expires evidence per round —
        // that is round discipline, not the re-fire bug.
        let w = r#"{"ts":1,"event":"demo.gather.replied","role":"kade","card_id":3305,"trace_id":"t","peer":"wren","round":"r1","note":""}"#;
        assert_eq!(nudge_targets(w, 3305, "kade", "r2"), vec!["wren", "silas"]);
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
        assert_eq!(gathers_missing(&w, 3352, "wren", "r1").len(), 2, "card 33520's gathers must not satisfy 3352");
    }

    #[test]
    fn gates_missing_ignores_other_cards_comma_terminated() {
        // a full gate set for card 31 must NOT satisfy card 3 (anti #3/#31 collision).
        let w = REQUIRED_GATES
            .iter()
            .map(|g| gate_line(31, g))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(gates_missing(&w, 3, "r1").len(), 5, "card 31's gates must not satisfy card 3");
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

    #[test]
    fn test_result_recorded_none_when_absent() {
        assert_eq!(test_result_recorded("", 3263), None);
    }

    #[test]
    fn test_result_recorded_ignores_other_cards() {
        assert_eq!(test_result_recorded(&test_result_line(31, "pass"), 3), None);
    }

}
