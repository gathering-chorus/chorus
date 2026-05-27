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

/// Which of the five role gates are absent from the card body. Ported from /demo
/// Step 2 (grep `gate:<g>-pass`). demo requires the chain complete before the act.
pub fn gates_missing(card_view: &str) -> Vec<&'static str> {
    // Case-insensitive: roles post `gate:code-pass` or `gate:code-PASS` interchangeably;
    // both must be recognized. Lowercase the view once, compare against lowercase needles.
    let v = card_view.to_lowercase();
    ["product", "code", "quality", "arch", "ops"]
        .iter()
        .filter(|g| !v.contains(&format!("gate:{}-pass", g)))
        .copied()
        .collect()
}

/// DEC-048 non-builder confirm: the confirming identity differs from the builder
/// (card owner). Mirrors werk-accept's can_accept authority axis — jeff is the
/// human authority (exempt), wren confirms others', nobody else confirms.
pub fn is_non_builder_confirm(confirmer: &str, owner: &str) -> bool {
    let c = confirmer.trim().to_lowercase();
    let o = owner.trim().to_lowercase();
    matches!(c.as_str(), "jeff" | "wren") && (c == "jeff" || c != o)
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

/// Run a CLI in an explicit working dir (so `gh` infers the repo from the werk remote).
fn run_in(dir: &str, cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(dir)
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

/// Best-effort gh per-card status `chorus/demo/<card>=success` on the werk HEAD.
/// The record is the logs + this status — no evidence token (card rule).
fn register_gh(werk_s: &str, card: u64, trace: &str) {
    if let Ok(sha) = run("git", &["-C", werk_s, "rev-parse", "HEAD"]) {
        let sha = sha.trim();
        let _ = run_in(
            werk_s,
            "gh",
            &[
                "api",
                "-X", "POST",
                &format!("repos/{{owner}}/{{repo}}/statuses/{}", sha),
                "-f", &format!("context=chorus/demo/{}", card),
                "-f", "state=success",
                "-f", &format!("description=demo trace {}", trace),
            ],
        );
    }
}

// --- ported from demo_preflight.rs (#1657) + preflight.sh + #2897 trace ---

/// Post the `demo:preflight-pass` card comment — the SINGLE gate-evidence
/// (#2910) that done-gate.sh / accept_gate look for at /acp time. Without
/// this, /acp refuses with "no demo evidence". This IS demo's gate output.
fn post_preflight_evidence(card: u64, role: &str, checked: usize, total: usize, trace: &str) -> R<()> {
    let comment = format!(
        "demo:preflight-pass ac={}/{} — {} (werk-demo, trace {})",
        checked, total, role, trace
    );
    run("cards", &["comment", &card.to_string(), &comment])
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

/// Step 3 hard gate. Skipped for `type:swat` cards (crisis exemption per the
/// old skill). app-affecting → --all; non-code cards skip.
fn run_smoke_check(home: &Path, card_view: &str) -> R<()> {
    if card_view.contains("type:swat") {
        return Ok(()); // crisis exemption
    }
    if !card_view.contains("type:fix")
        && !card_view.contains("type:enhance")
        && !card_view.contains("type:new")
    {
        return Ok(()); // non-code (chore/docs/decisions) — skip smoke
    }
    let script = home.join("platform/scripts/smoke-check.sh");
    let s = path(&script)?;
    let out = Command::new("bash")
        .args([s, "--all"])
        .output()
        .map_err(|e| format!("smoke-check failed to start: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "smoke-check failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

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

/// Gate → owning role map. Wren owns product, Kade owns code+quality,
/// Silas owns arch+ops. Used by the gate-request fan-out + refusal naming.
pub fn gate_owner(gate: &str) -> &'static str {
    match gate {
        "product" => "wren",
        "code" | "quality" => "kade",
        "arch" | "ops" => "silas",
        _ => "unknown",
    }
}

/// Send a gate-request nudge to `to` via the chorus_nudge_message MCP path.
/// Neutral framing: pointers and ask, no editorializing. Sender, gates needed,
/// then "read the card, read the code, run the gates" instruction, plus ack
/// expected. No "narrow/clean/delivered" pre-framing — reviewer forms their
/// own read.
fn send_gate_request_nudge(from: &str, to: &str, card: u64, gates: &[String], trace: &str) -> R<()> {
    let mcp_url = std::env::var("CHORUS_MCP_URL")
        .unwrap_or_else(|_| "http://localhost:3341/mcp".to_string());
    let gate_list = gates.iter().map(|g| format!("gate:{}", g)).collect::<Vec<_>>().join(" + ");
    let msg = format!(
        "[gate #{} — ACK REQUIRED]\\nFrom: {}\\nNeeds: {} (your lanes)\\nRead the card. Read the code. Run the gates.\\nAck: substantive reply or blocked-on-X within 10 min.",
        card, from, gate_list
    );
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"chorus_nudge_message","arguments":{{"to":"{}","message":"{}"}}}}}}"#,
        to, msg
    );
    run("curl", &[
        "-s", "-f", "-X", "POST",
        &mcp_url,
        "-H", "Content-Type: application/json",
        "-H", "Accept: application/json, text/event-stream",
        "-H", &format!("X-Chorus-Role: {}", from),
        "-H", &format!("X-Chorus-Trace-Id: {}", trace),
        "-d", &body,
    ]).map(|_| ())
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
    // Neutral framing — pointers + ask, no editorializing that biases the
    // reply. Sender + ack-required up front; "read the card and the code"
    // instruction (recipient forms their own read); the 3 skill questions
    // as the actual ask. No "before /acp" pressure, no "narrow/clean/delivered"
    // pre-framing — those just inherit my satisfaction to the reviewer.
    let msg = format!(
        "[feedback #{} — ACK REQUIRED]\\nFrom: {}\\nRead the card. Read the code. Then reply.\\n(1) How does this impact your products?\\n(2) How does it impact your users?\\n(3) Am I over-building or under-planning?\\nAck: substantive reply or blocked-on-X within 10 min.",
        card, from
    );
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

/// Check if a peer role has engaged with this card's demo. Cheap heuristic:
/// search chorus-api's spine for recent activity from `other` on `card`. If
/// the response contains both, treat as engaged. Stays zero-dep (no JSON
/// parsing). Used post-comment-window to detect silent peers (#3100 AC #2).
fn peer_engaged(other: &str, card: u64) -> bool {
    let q = format!("card_id={}+role={}", card, other);
    let check = run("curl", &[
        "-sS", "-G",
        "http://localhost:3340/api/chorus/search",
        "--data-urlencode", &format!("q={}", q),
        "--data-urlencode", "limit=3",
    ]).unwrap_or_default();
    check.contains(&format!("\"role\":\"{}\"", other))
        && check.contains(&card.to_string())
}

/// Step 5: signal — cards demo + spine event + Bridge post + feedback nudges.
/// All four are best-effort (the act has already gated; signal is the announcement,
/// not a gate). Bridge + nudges are HTTP POSTs to localhost services (zero-dep:
/// curl as a subprocess, mirroring the verb-contract).
fn signal(card: u64, role: &str, home: &Path, trace: &str) {
    let card_s = card.to_string();
    // board demo signal
    let _ = run("cards", &["demo", &card_s]);
    emit_spine(home, "card.demo.started", role, card, trace);

    // Bridge post (localhost:3470 — Jeff's center panel)
    let bridge_body = format!(
        r#"{{"from":"{}","text":"[demo] #{} — werk-demo: act ran live (build → deploy → verify)"}}"#,
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

/// Testable core — all inputs explicit. Validate → AC-preflight → gate-chain →
/// build → deploy → verify → record. The act (build/deploy) runs the werk code
/// live in the one prod env via the shipped verbs; acp is what lands it in main.
pub fn demo(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = env::var("CHORUS_TRACE_ID").unwrap_or_else(|_| trace_id());
    jsonl(home, role, card, &trace, "demo.started", "");

    let card_s = card.to_string();

    // Step 1: validate — exists + WIP/Now.
    let cj = run("cards", &["view", &card_s, "--json"])
        .map_err(|e| format!("validate: cannot read card #{}: {}", card, e))?;
    let status = json_str_field(&cj, "status").unwrap_or_default();
    if status != "WIP" && status != "Now" {
        jsonl(home, role, card, &trace, "demo.refused", ",\"reason\":\"wrong-status\"");
        return Err(format!("#{} is {} — must be WIP/Now to demo", card, status));
    }

    // Step 1.5: AC pre-flight — all AC checked (uses the human view for checkboxes).
    let cv = run("cards", &["view", &card_s])?;
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
    post_preflight_evidence(card, role, checked, total, &trace)?;
    write_trace_file(card, &trace);
    jsonl(home, role, card, &trace, "demo.preflight.passed", &format!(",\"ac\":\"{}/{}\"", checked, total));

    // Step 2: gate chain — all five role gates present. AC #1 + gate-request
    // fan-out: emit demo.gate.requested, detect missing gates, and on detection
    // FAN OUT a single concurrent nudge round to all gate-owners (no sequential
    // chain — sender does not handoff to a peer to nudge another peer). Wait
    // CHORUS_DEMO_GATE_WAIT_SECS for gates to land, then re-check. Refuse with
    // owner-set if still missing. Builder's own gate cannot be substituted by
    // a nudge — they owe it themselves; refuse fast on that case.
    emit_spine(home, "demo.gate.requested", role, card, &trace);
    let missing = gates_missing(&cv);
    if !missing.is_empty() {
        // Builder owes their own gate first — no self-nudge possible.
        let self_owed: Vec<&&str> = missing.iter().filter(|g| gate_owner(g) == role).collect();
        if !self_owed.is_empty() {
            let owed_str: Vec<String> = self_owed.iter().map(|g| format!("{}({})", g, role)).collect();
            jsonl(home, role, card, &trace, "demo.refused",
                  &format!(",\"reason\":\"self-gate-missing\",\"owed\":\"{}\"", owed_str.join(",")));
            return Err(format!("#{} you owe your own gate first: {}", card, owed_str.join(", ")));
        }
        // All other missing gates owed by peers — fan out one nudge per owner.
        // Collect unique owners (Kade owns both code+quality, Silas owns both
        // arch+ops, so deduping gives 1-2 outbound nudges, never 3+).
        let mut owners_to_nudge: Vec<&str> = Vec::new();
        for g in &missing {
            let owner = gate_owner(g);
            if owner != role && !owners_to_nudge.contains(&owner) {
                owners_to_nudge.push(owner);
            }
        }
        for owner in &owners_to_nudge {
            let owner_gates: Vec<String> = missing.iter()
                .filter(|g| gate_owner(g) == *owner)
                .map(|g| g.to_string())
                .collect();
            if let Err(e) = send_gate_request_nudge(role, owner, card, &owner_gates, &trace) {
                jsonl(home, role, card, &trace, "demo.gate.nudge_failed",
                      &format!(",\"to\":\"{}\",\"reason\":\"{}\"", owner, e.replace('"', "'")));
            } else {
                jsonl(home, role, card, &trace, "demo.gate.nudge_sent",
                      &format!(",\"to\":\"{}\",\"gates\":\"{}\"", owner, owner_gates.join(",")));
            }
        }
        // Wait once for all gates to land (fan-out semantic: no chained handoffs).
        let gate_wait: u64 = std::env::var("CHORUS_DEMO_GATE_WAIT_SECS")
            .ok().and_then(|s| s.parse().ok()).unwrap_or(120);
        jsonl(home, role, card, &trace, "demo.gate.waiting",
              &format!(",\"secs\":{},\"nudged\":\"{}\"", gate_wait, owners_to_nudge.join(",")));
        std::thread::sleep(std::time::Duration::from_secs(gate_wait));
        // Re-check after the wait — re-read the card view; if all in, proceed.
        let cv2 = run("cards", &["view", &card_s])?;
        let still_missing = gates_missing(&cv2);
        if !still_missing.is_empty() {
            let owners: Vec<String> = still_missing.iter()
                .map(|g| format!("{}({})", g, gate_owner(g))).collect();
            jsonl(home, role, card, &trace, "demo.refused",
                  &format!(",\"reason\":\"gates-still-missing-after-wait\",\"owed_by\":\"{}\"",
                           owners.join(",")));
            return Err(format!("#{} gate chain still incomplete after {}s — owed by: {}",
                               card, gate_wait, owners.join(", ")));
        }
    }
    emit_spine(home, "demo.gate.passed", role, card, &trace);

    // Step 3: smoke check (hard gate — type:swat exempt, non-code skipped, per skill).
    run_smoke_check(home, &cv).inspect_err(|_e| {
        jsonl(home, role, card, &trace, "demo.refused", ",\"reason\":\"smoke-failed\"");
    })?;
    jsonl(home, role, card, &trace, "demo.smoke.passed", "");

    // Step 5: signal — board demo + spine event + Bridge + feedback nudges (best-effort,
    // the act has already gated; this announces). Step 4 stakes-brief is human-driven
    // content; demo-v2 records it in spine events, not as a separate gate.
    signal(card, role, home, &trace);
    jsonl(home, role, card, &trace, "demo.signal.completed", "");

    // The ACT: build → env-up → verify. Demo runs in the role's WERK VARIANT
    // (chorus-api/mcp on per-role ports), NOT canonical prod. /acp's accept lane
    // still runs `werk-deploy <card>` (canonical) post-demo — two distinct calls
    // for two distinct purposes (#3098 closes the demo=prod consumer gap on
    // #3092's env-up primitive).
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?;
    run("werk-build", &[&card_s]).map_err(|e| format!("demo build: {}", e))?;
    jsonl(home, role, card, &trace, "demo.built", "");
    // werk-deploy env-up brings up the role's chorus-api + chorus-mcp variants
    // from werk source (per-role ports, isolated from canonical), smokes them,
    // writes activation markers. Idempotent — re-running refreshes against
    // current werk dist. State (DB/Fuseki/Loki/Vikunja) is shared by design.
    run("werk-deploy", &["env-up", role, &card_s])
        .map_err(|e| format!("demo deploy/verify: {}", e))?;
    jsonl(home, role, card, &trace, "demo.deployed", "");

    // Pair with #3101: after env-up + (when #3101 lands) the CLI-verb wrapper
    // resolves role-slot-first, install CLI-verb crates from the card to the
    // role's WERK_<ROLE>_BIN so the demo-er actually runs the new code
    // mid-demo. Without this, build+deploy ran but the new binary never lands
    // where PATH can find it — the whole-point-is-testing gap Silas named.
    if let Err(e) = run("werk-deploy", &[&card_s, role, "--target", "werk"]) {
        // Soft-fail: not every card has a CLI-verb change. werk-deploy --target
        // werk should be a no-op for non-CliVerb diffs. If it actually fails on
        // a CliVerb card, surface the reason; don't abort the demo.
        jsonl(home, role, card, &trace, "demo.cliverb_install.skipped_or_failed",
              &format!(",\"reason\":\"{}\"", e.replace('"', "'")));
    } else {
        jsonl(home, role, card, &trace, "demo.cliverb_installed", "");
    }

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

    // The accept_gate evidence event — without this, /acp refuses at accept time
    // (mirrors show-gate.sh emitting demo.show.completed on a successful demo).
    emit_spine(home, "demo.show.completed", role, card, &trace);
    register_gh(werk_s, card, &trace);

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
        r#"{{"from":"{}","text":"━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n🎬 [DEMO READY FOR JEFF] — card #{}\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\nVariant up: {}\nGates green; act complete; awaiting your eyes.\n→ React with questions, check the variant, or /acp when satisfied."}}"#,
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

    // #3100 AC #5 — comment window. After the announce, Jeff gets a real
    // pause to react/comment before the demo is "done." Binary sleeps for the
    // configured window so the BUILDER AGENT does not jump into /acp solicitation
    // prose. Default 60s; CHORUS_DEMO_COMMENT_WINDOW_SECS overrides for tests.
    // demo.awaiting_comment fires at window start; demo.comment_window_closed
    // at window end. AC #6 (no premature /acp begging) follows naturally —
    // the binary doesn't return until the window closes, so the agent can't
    // prose-prompt /acp before that.
    let window_secs: u64 = std::env::var("CHORUS_DEMO_COMMENT_WINDOW_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    jsonl(home, role, card, &trace, "demo.awaiting_comment",
          &format!(",\"window_secs\":{}", window_secs));
    std::thread::sleep(std::time::Duration::from_secs(window_secs));
    jsonl(home, role, card, &trace, "demo.comment_window_closed", "");

    // #3100 AC #2 + #3 — feedback interaction + no-intervention.
    // After the comment window, check each peer role's spine activity for a
    // response. If silent: emit demo.feedback.unacked, re-nudge once via the
    // same MCP path, sleep the ack-window, re-check. If STILL silent, emit
    // demo.feedback.escalate + post a Bridge announce to Jeff so the substrate
    // (not Jeff) chases the stall. Implements the DEC-107 + /demo-skill
    // ack discipline structurally instead of as prose.
    let ack_window_secs: u64 = std::env::var("CHORUS_DEMO_ACK_WINDOW_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let mut unacked_round1: Vec<&str> = Vec::new();
    for other in ["wren", "silas", "kade"].iter().filter(|r| **r != role) {
        if !peer_engaged(other, card) {
            jsonl(home, role, card, &trace, "demo.feedback.unacked",
                  &format!(",\"from\":\"{}\",\"round\":1", other));
            // Re-nudge once via the same MCP path used in signal().
            if let Err(e) = send_mcp_nudge(role, other, card, &trace) {
                jsonl(home, role, card, &trace, "demo.renudge.failed",
                      &format!(",\"to\":\"{}\",\"reason\":\"{}\"", other, e.replace('"', "'")));
            } else {
                jsonl(home, role, card, &trace, "demo.renudge.sent",
                      &format!(",\"to\":\"{}\"", other));
            }
            unacked_round1.push(other);
        }
    }
    // Wait the ack-window then re-check the silent peers. Anyone still silent
    // escalates — both as a spine event AND as a Bridge announce to Jeff.
    if !unacked_round1.is_empty() {
        std::thread::sleep(std::time::Duration::from_secs(ack_window_secs));
        for other in &unacked_round1 {
            if !peer_engaged(other, card) {
                jsonl(home, role, card, &trace, "demo.feedback.escalate",
                      &format!(",\"from\":\"{}\",\"reason\":\"unacked-after-renudge\"", other));
                // AC #3 — surface to Jeff via Bridge so he doesn't have to notice silently.
                let escalate_body = format!(
                    r#"{{"from":"{}","text":"⚠️  [FEEDBACK STALL] #{} — {} did not ack the feedback nudge after re-nudge; substrate has escalated. Either chase or /acp anyway if you're comfortable."}}"#,
                    role, card, other
                );
                let _ = run("curl", &[
                    "-s", "-f", "-X", "POST",
                    "http://localhost:3470/api/message",
                    "-H", "Content-Type: application/json",
                    "-d", &escalate_body,
                ]);
            }
        }
    }

    jsonl(home, role, card, &trace, "demo.completed", "");
    Ok(format!("demo #{} — built, deployed, verified live ({}/{} AC, gates green) — ready for review (commented {}s)", card, checked, total, window_secs))
}

/// CLI shim: parse args/env only, then call the testable core (blueprint pattern).
pub fn run_demo() -> R<String> {
    let card: u64 = env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("usage: werk-demo <card-id>")?;
    let role = env::var("DEPLOY_ROLE").unwrap_or_default();
    if role.trim().is_empty() {
        return Err("DEPLOY_ROLE unset — cannot demo without a role".to_string());
    }
    let home = env::var("CHORUS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&env::var("HOME").unwrap_or_default()).join("CascadeProjects/chorus"));
    let werk_base = env::var("CHORUS_WERK_BASE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.parent().map(|p| p.join("chorus-werk")).unwrap_or_else(|| home.join("../chorus-werk")));
    demo(card, role.trim(), &home, &werk_base)
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
    fn gates_missing_lists_absent_gates() {
        let v = "gate:product-pass — Wren\ngate:code-pass — Kade\ngate:quality-pass — Kade";
        let missing = gates_missing(v);
        assert_eq!(missing, vec!["arch", "ops"]);
    }

    #[test]
    fn gates_missing_empty_when_all_present() {
        let v = "gate:product-pass gate:code-pass gate:quality-pass gate:arch-pass gate:ops-pass";
        assert!(gates_missing(v).is_empty());
    }

    #[test]
    fn dec048_jeff_confirms_anything_wren_confirms_others_not_own() {
        assert!(is_non_builder_confirm("jeff", "wren")); // human authority
        assert!(is_non_builder_confirm("jeff", "jeff"));
        assert!(is_non_builder_confirm("wren", "kade")); // wren confirms others'
        assert!(!is_non_builder_confirm("wren", "wren")); // not her own
        assert!(!is_non_builder_confirm("kade", "kade")); // builder can't self-confirm
        assert!(!is_non_builder_confirm("kade", "silas")); // non-authority
    }

    #[test]
    fn json_str_field_tolerates_pretty_print() {
        assert_eq!(json_str_field("{ \"status\" : \"WIP\" }", "status"), Some("WIP".to_string()));
        assert_eq!(json_str_field("{\"status\":\"Done\"}", "status"), Some("Done".to_string()));
    }
}
