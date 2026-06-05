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

/// #3237 — the gates that must have run before a demo verdict can be recorded.
/// Gates moved to the /demo SKILL layer (#3116) as LLM subagents; this is the
/// enforcement contract that makes them non-optional — the binary refuses a
/// verdict until each has left a demo.gate.result in the witness.
pub const REQUIRED_GATES: [&str; 5] = ["product", "code", "quality", "arch", "ops"];

/// Given the werk-demo witness content + a card id, return the required gates
/// with NO demo.gate.result recorded for the card. Empty = the full gate gather
/// ran → a verdict may be recorded. card_id is matched comma-terminated so a
/// gate for #31 can't satisfy #3 (the #3116/#31160 collision class).
pub fn gates_missing(witness: &str, card: u64) -> Vec<&'static str> {
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
            })
        })
        .collect()
}

/// #3237 — Jeff's three-way demo decision: the ONLY human input to the blocking
/// demo step. go = accept → merge; no-go = reject → unpull; more = iterate.
/// "go" IS the DEC-048 accept — there is no separate werk-accept step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Go,
    NoGo,
    More,
}

impl Decision {
    /// The act ↔ werk-demo exit-code contract (locked with Kade): go → 0 (act
    /// continues to merge); no-go | more → 2 (act stops, nothing merged) — a
    /// CLEAN/intended stop, a distinct code from a real error (1) so the witness
    /// never reads an intended stop as red (the werk.failed-reads-red trap).
    pub fn exit_code(&self) -> i32 {
        match self {
            Decision::Go => 0,
            Decision::NoGo | Decision::More => 2,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Decision::Go => "go",
            Decision::NoGo => "no-go",
            Decision::More => "more",
        }
    }

    fn parse(s: &str) -> Option<Decision> {
        match s {
            "go" => Some(Decision::Go),
            "no-go" => Some(Decision::NoGo),
            "more" => Some(Decision::More),
            _ => None,
        }
    }
}

/// Scan the witness for the LATEST `demo.decision` recorded for this card and
/// return Jeff's go/no-go/more. None = no decision yet → the binary keeps
/// blocking. card is matched comma-terminated (`"card_id":N,`) so card 31's
/// decision can't satisfy card 3 — same anti-collision rule as gates_missing.
pub fn read_decision(witness: &str, card: u64) -> Option<Decision> {
    let card_key = format!("\"card_id\":{},", card);
    witness
        .lines()
        .rev()
        .filter(|l| l.contains("\"event\":\"demo.decision\"") && l.contains(&card_key))
        .find_map(|l| json_str_field(l, "decision").and_then(|d| Decision::parse(&d)))
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
fn send_mcp_nudge(from: &str, other: &str, card: u64, trace: &str) -> R<()> {
    let mcp_url = std::env::var("CHORUS_MCP_URL")
        .unwrap_or_else(|_| "http://localhost:3341/mcp".to_string());
    let msg = feedback_message(card, from);
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"chorus_nudge_message","arguments":{{"to":"{}","message":"{}"}}}}}}"#,
        other, msg
    );
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
fn signal(card: u64, role: &str, home: &Path, trace: &str) {
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
    for other in ["wren", "silas", "kade"].iter().filter(|r| **r != role) {
        if let Err(e) = send_mcp_nudge(role, other, card, trace) {
            jsonl(home, role, card, trace, "demo.nudge.failed",
                  &format!(",\"to\":\"{}\",\"reason\":\"{}\"", other, e.replace('"', "'")));
        }
    }
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
        return Err(format!("#{} is {} — must be WIP/Now to demo", card, status));
    }

    // Step 1.5: AC pre-flight — all AC checked (uses the human view for checkboxes).
    let cv = run(&script_path(home, "cards"), &["view", &card_s])?;
    let (checked, total) = ac_counts(&cv);
    if total == 0 {
        jsonl(home, role, card, &trace, "demo.refused", ",\"reason\":\"no-ac\"");
        return Err(format!("#{} has no acceptance criteria", card));
    }
    if checked < total {
        jsonl(home, role, card, &trace, "demo.refused", ",\"reason\":\"ac-incomplete\"");
        return Err(format!("#{}: {}/{} AC checked — complete before demo", card, checked, total));
    }
    // Post the SINGLE gate-evidence comment + trace file (#2910 / #2897). Without
    // these, /acp will refuse "no demo evidence" via done-gate.sh / accept_gate.
    post_preflight_evidence(home, card, role, checked, total, &trace)?;
    write_trace_file(card, &trace);
    jsonl(home, role, card, &trace, "demo.preflight.passed", &format!(",\"ac\":\"{}/{}\"", checked, total));

    // #3116 — the GATE step moves to the /demo SKILL layer. The demoer initiates
    // the 5 gates as subagents (an LLM gate-review can't run in this zero-dep
    // binary) and routes each result to its owning role for REVIEW. The old
    // go-run-your-gate nudge relay + the in-binary gate-chain wait are retired
    // (the agents-grading-agents medium was the waste, not the gates). Smoke
    // folds into the machine prover. The binary no longer blocks on gate comments.
    emit_spine(home, "demo.gate.delegated", role, card, &trace);

    // Step 5: signal — board demo + spine event + Bridge + feedback nudges (best-effort,
    // the act has already gated; this announces). Step 4 stakes-brief is human-driven
    // content; demo-v2 records it in spine events, not as a separate gate.
    signal(card, role, home, &trace);
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

    // #3237 — GATE-EVIDENCE ENFORCEMENT (fires before the human pause). #3116
    // moved the 5 gates to the /demo skill (LLM subagents); the binary refuses to
    // honor a decision unless every required gate left a demo.gate.result in the
    // witness for this card — so neither the /demo skill nor the act path can
    // merge un-gated. A clean, intended stop (exit 2: "go run the gates"), not error.
    let witness = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
    let missing = gates_missing(&witness, card);
    if !missing.is_empty() {
        jsonl(home, role, card, &trace, "demo.refused",
              &format!(",\"reason\":\"gates-missing\",\"missing\":\"{}\"", missing.join(",")));
        return Ok(DemoOutcome {
            message: format!(
                "#{} demo refused — gates not run: {}. Run the /demo gate subagents \
                 (each records via `werk-demo gate {} <gate> pass|fail`) before a decision.",
                card, missing.join(", "), card
            ),
            exit: 2,
        });
    }

    // #3237 — THE BLOCKING HUMAN STEP. act calls werk-demo synchronously and blocks
    // on its exit code, so the entire human pause lives HERE — no approval gate, no
    // pause/resume machinery. Gates have run; now block until Jeff records his ONE
    // decision (go / no-go / more) in the witness. "go" IS the DEC-048 accept;
    // there is NO separate werk-accept. Headless-safe: poll the witness file, no
    // stdin/TTY (werk-demo runs in the act/daemon context). A max-block timeout
    // fails SAFE to "more" (exit 2) — a timer never stands in for Jeff's hand.
    let max_block_secs: u64 = std::env::var("CHORUS_DEMO_MAX_BLOCK_SECS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(1800);
    let poll_secs: u64 = std::env::var("CHORUS_DEMO_POLL_SECS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(2);
    jsonl(home, role, card, &trace, "demo.awaiting_decision",
          &format!(",\"max_block_secs\":{}", max_block_secs));

    let started = std::time::Instant::now();
    let decision = loop {
        let w = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
        if let Some(d) = read_decision(&w, card) {
            break d;
        }
        if started.elapsed().as_secs() >= max_block_secs {
            jsonl(home, role, card, &trace, "demo.decision.timeout", ",\"fallback\":\"more\"");
            break Decision::More;
        }
        sleep(std::time::Duration::from_secs(poll_secs.max(1)));
    };
    jsonl(home, role, card, &trace, "demo.decision.honored",
          &format!(",\"decision\":\"{}\"", decision.label()));

    // go = accept → record the demo.verdict pass (the merge-gating evidence).
    // no-go / more record demo.stopped (no verdict = no merge). act reads ONLY the
    // exit code: go→0 (continue to merge), no-go|more→2 (stop, nothing merged).
    let message = match decision {
        Decision::Go => {
            let prover = std::env::var("CHORUS_DEMO_PROVER").unwrap_or_else(|_| "stub".to_string());
            let auto = prover == "stub";
            let verdict_extra = format!(
                ",\"verdict\":\"pass\",\"prover\":\"{}\",\"auto\":{},\"ac\":\"{}/{}\",\"decision\":\"go\"",
                prover, auto, checked, total
            );
            jsonl(home, role, card, &trace, "demo.verdict", &verdict_extra);
            emit_spine(home, "demo.verdict", role, card, &trace);
            format!(
                "demo #{} — GO/accepted ({}/{} AC); verdict recorded (prover={}). act continues to merge.",
                card, checked, total, prover
            )
        }
        Decision::NoGo => {
            jsonl(home, role, card, &trace, "demo.stopped", ",\"decision\":\"no-go\"");
            format!("demo #{} — NO-GO (rejected). Nothing merged; card returns to Next.", card)
        }
        Decision::More => {
            jsonl(home, role, card, &trace, "demo.stopped", ",\"decision\":\"more\"");
            format!("demo #{} — MORE (iterate). Nothing merged; werk preserved.", card)
        }
    };

    jsonl(home, role, card, &trace, "demo.completed",
          &format!(",\"decision\":\"{}\"", decision.label()));
    Ok(DemoOutcome { message, exit: decision.exit_code() })
}

/// #3237 — record one gate's result into the witness so the verdict step can
/// verify the gate gather ran. Called by the /demo skill's gate subagents via
/// `werk-demo gate <card> <gate> <result>`. The witness is the same
/// ops/logs/werk-demo.jsonl the verdict + werk-accept read — one evidence file.
fn record_gate(home: &Path, role: &str, card: u64, gate: &str, result: &str) {
    let trace = env::var("CHORUS_TRACE_ID").unwrap_or_else(|_| trace_id());
    jsonl(home, role, card, &trace, "demo.gate.result",
          &format!(",\"gate\":\"{}\",\"result\":\"{}\"", gate, result));
    emit_spine(home, "demo.gate.result", role, card, &trace);
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
        record_gate(&home, role.trim(), card, &gate, &result);
        return Ok(DemoOutcome {
            message: format!("gate {} recorded for #{}: {}", gate, card, result),
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
            r#"{{"ts":1,"event":"demo.gate.result","role":"wren","card_id":{},"trace_id":"t","gate":"{}","result":"pass"}}"#,
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
        assert!(gates_missing(&w, 3237).is_empty(), "all 5 gates present → none missing");
    }

    #[test]
    fn gates_missing_lists_absent_gates() {
        let w = format!("{}\n{}", gate_line(3237, "product"), gate_line(3237, "code"));
        assert_eq!(gates_missing(&w, 3237), vec!["quality", "arch", "ops"]);
    }

    #[test]
    fn gates_missing_all_when_witness_empty() {
        assert_eq!(gates_missing("", 3237).len(), 5);
    }

    #[test]
    fn gates_missing_ignores_other_cards_comma_terminated() {
        // a full gate set for card 31 must NOT satisfy card 3 (anti #3/#31 collision).
        let w = REQUIRED_GATES
            .iter()
            .map(|g| gate_line(31, g))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(gates_missing(&w, 3).len(), 5, "card 31's gates must not satisfy card 3");
    }
}
