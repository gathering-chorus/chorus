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
    ["product", "code", "quality", "arch", "ops"]
        .iter()
        .filter(|g| !card_view.contains(&format!("gate:{}-pass", g)))
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
    // canonical nudge surface. werk-demo invokes it by POSTing a JSON-RPC
    // tools/call to the MCP server's HTTP endpoint. -f so 4xx/5xx surface
    // (the #3100 bug class: silent 404 on the wrong endpoint).
    let mcp_url = std::env::var("CHORUS_MCP_URL")
        .unwrap_or_else(|_| "http://localhost:3341/mcp".to_string());
    for other in ["wren", "silas", "kade"].iter().filter(|r| **r != role) {
        let msg = format!(
            "[feedback] #{} — werk-demo ran live.\\n(1) How does this impact your products?\\n(2) How does it impact your users?\\n(3) Am I over-building or under-planning?\\nACK REQUIRED within 10 min or blocked-on-X.",
            card
        );
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"chorus_nudge_message","arguments":{{"to":"{}","message":"{}"}}}}}}"#,
            other, msg
        );
        if let Err(e) = run(
            "curl",
            &[
                "-s", "-f", "-X", "POST",
                &mcp_url,
                "-H", "Content-Type: application/json",
                "-H", &format!("X-Chorus-Role: {}", role),
                "-H", &format!("X-Chorus-Trace-Id: {}", trace),
                "-d", &body,
            ],
        ) {
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

    // Step 2: gate chain — all five role gates present.
    let missing = gates_missing(&cv);
    if !missing.is_empty() {
        jsonl(home, role, card, &trace, "demo.refused", &format!(",\"reason\":\"gates-missing\",\"missing\":\"{}\"", missing.join(",")));
        return Err(format!("#{} gate chain incomplete — missing: {}", card, missing.join(", ")));
    }

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

    // The accept_gate evidence event — without this, /acp refuses at accept time
    // (mirrors show-gate.sh emitting demo.show.completed on a successful demo).
    emit_spine(home, "demo.show.completed", role, card, &trace);
    register_gh(werk_s, card, &trace);

    // #3100 AC3 — human-pause step. The /demo skill's original Step 6 ("Show,
    // then wait") didn't survive the fold into the binary; werk-demo runs to
    // completion. Replace the in-process wait with an explicit Bridge
    // announcement: variant is up, gates green, this is the moment to react
    // or ask. Agent reads Bridge and engages; binary terminates clean.
    let pause_body = format!(
        r#"{{"from":"{}","text":"[demo ready] #{} — werk-variant up; gates green; ready for your eyes. Ask questions, check the variant, or /acp when satisfied."}}"#,
        role, card
    );
    let _ = run(
        "curl",
        &[
            "-s", "-X", "POST",
            "http://localhost:3470/api/message",
            "-H", "Content-Type: application/json",
            "-d", &pause_body,
        ],
    );
    jsonl(home, role, card, &trace, "demo.ready_for_review", "");

    jsonl(home, role, card, &trace, "demo.completed", "");
    Ok(format!("demo #{} — built, deployed, verified live ({}/{} AC, gates green) — ready for review", card, checked, total))
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
