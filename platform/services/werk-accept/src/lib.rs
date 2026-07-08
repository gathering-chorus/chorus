//! werk-accept — `/werk/accept` v2 logic (atomic EXIT-FINALIZE verb, #3057).
//!
//! Self-contained: std only + a direct libc `flock` extern; calls `git`, `gh`,
//! `cards`, `chorus-log`, `chorus-werk` as subprocesses. No dependency on any
//! other chorus code.
//!
//! ATOMIC VERB — accept ONLY, sequenced LAST (pull→commit→push→build→deploy→verify
//! →merge→accept). It FINALIZES a proven, ALREADY-MERGED card: flip the
//! card WIP → Done (emitting `card.accepted` — the ONLY Done path), close the
//! branch + werk, and stamp gh `chorus/accept/<card>`. It NEVER re-gates at entry
//! (that's pull's job, ADR-032 §4); it only validates its EXIT preconditions.
//!
//! FINALIZE-ONLY (#3175): accept does NOT merge — werk-merge is the ONE merge
//! mechanism, run earlier in the sequence. The one product concern is the AUTHORITY GATE
//! (DEC-048): only Wren/Jeff may finalize, and a builder may NEVER self-accept its
//! own card. That gate is `can_accept()` — pure, exhaustively unit-tested.
//!
//! - All-or-nothing finalize, serialized under one flock so concurrent board flips
//!   can't race. Steps are idempotent so a re-run completes a partial finalize
//!   (already-Done → no-op success).
//! - JSONL witness per step: best-effort, NEVER affects the operation.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// #3513 — the ONE shared failure classifier (failure_class / fail_extra).
include!("../../shared/failure_class.rs");

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}
const LOCK_EX_NB: i32 = 0x02 | 0x04;
const LOCK_UN: i32 = 0x08;

pub type R<T> = Result<T, String>;

// --- pure helpers (unit-tested) ---

// #3331 — branch_name REMOVED: zero callers since #3175 made accept finalize-only
// (branch close is chorus-werk remove's). Confirmed semantically: ast-grep found no
// `branch_name(...)` call anywhere in the crate; the only other mention was the
// units.rs import (cleaned with it).

/// AUTHORITY GATE (DEC-048). Only Wren/Jeff may finalize a card. Pure so the rule
/// is exhaustively testable and lives in exactly one place. Wren's spec, exact:
/// - `jeff`  → accept ANYTHING, incl. a jeff-owned card. The human final-authority
///   has no higher authority to protect against, so the self-accept rule doesn't
///   apply to it (#3057 gate-arch finding — Silas).
/// - `wren`  → accept any card EXCEPT her own (no grading your own homework, #2979).
/// - anyone else → never (kade/silas/unset all refuse).
pub fn can_accept(accepter: &str, owner: &str) -> bool {
    match accepter {
        "jeff" => true,
        "wren" => accepter != owner,
        _ => false,
    }
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

/// #3237 — build the `demo.decision` line werk-demo's read_decision polls for in
/// ops/logs/werk-demo.jsonl. Wraps jsonl_line so card_id stays comma-terminated
/// (`"card_id":N,`) — the byte-exact match that blocks the #3/#31 collision. The
/// role field carries the accepter (who rendered the decision); decision is
/// go | no-go | more. The go-signal (werk-accept) and werk-do-more both emit through
/// this one builder so the witnessed shape can't drift between the two verbs.
pub fn demo_decision_line(ts: u128, card: u64, decision: &str, accepter: &str, trace: &str) -> String {
    jsonl_line(ts, "demo.decision", accepter, card, trace, &format!(",\"decision\":\"{}\"", decision))
}

/// Minimal whitespace-tolerant JSON string-field extractor (zero-dep). Same as the
/// pull blueprint — robust against `cards --json` pretty-printing.
pub fn json_str_field(json: &str, key: &str) -> Option<String> {
    let i = json.find(&format!("\"{}\"", key))?;
    let after_key = &json[i + key.len() + 2..];
    let colon = after_key.find(':')?;
    let after_colon = &after_key[colon + 1..];
    let q1 = after_colon.find('"')?;
    let val = &after_colon[q1 + 1..];
    let q2 = val.find('"')?;
    Some(val[..q2].to_string())
}

// #3499 — REMOVED: demo_presented(). It read the werk-demo witness for a
// `demo.presented` event and gated finalize on it (#3410 self-accept backstop).
// finalize no longer audits the demo step — the orchestrator owns ordering — so
// the check and its reader are gone.

// --- side-effecting helpers ---

fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-accept.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(jsonl_line(ts, event, role, card, trace, extra).as_bytes());
    }
}

fn run_in(dir: &str, cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd).args(args).current_dir(dir).output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{} {}: {}", cmd, args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run(cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd).args(args).output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{} {}: {}", cmd, args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// #3183 — resolve a chorus script (cards, chorus-log, chorus-werk) to its ABSOLUTE
/// path under $CHORUS_HOME/platform/scripts. werk-accept is exec'd by the chorus-mcp
/// daemon, whose PATH does NOT include platform/scripts, so bare-name lookups ("cards")
/// failed with "No such file or directory" — proven live accepting #3211, which is why
/// accepts fell back to `cards done`. Mirrors werk-pull's #3151 fix. (git/gh stay bare
/// — they're on the system PATH.)
pub fn script_path(home: &Path, name: &str) -> String {
    home.join("platform/scripts").join(name).to_string_lossy().into_owned()
}

/// #3183 — resolve a chorus-* BINARY (werk-deploy) to its absolute install path under
/// $CHORUS_BIN (default ~/.chorus/bin), same PATH-independence as script_path.
fn bin_path(name: &str) -> String {
    let dir = env::var("CHORUS_BIN")
        .unwrap_or_else(|_| format!("{}/.chorus/bin", env::var("HOME").unwrap_or_default()));
    format!("{}/{}", dir, name)
}

pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

pub fn lock(home: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = home.join(".git/chorus-accept.lock");
    let f = OpenOptions::new().create(true).write(true).truncate(false).open(&p)
        .map_err(|e| format!("cannot open lock {}: {}", p.display(), e))?;
    let start = Instant::now();
    loop {
        if unsafe { flock(f.as_raw_fd(), LOCK_EX_NB) } == 0 {
            return Ok(FlockGuard(f));
        }
        if start.elapsed() >= timeout {
            return Err("another accept holds the repo lock (timed out after 30s)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// Entry for werk-accept (#3237): the GO-SIGNAL. `werk-accept <card> <role>`; the
/// #3298 — parse accept args + recognize `--atomic` ANYWHERE (the standalone accept
/// door; the #3296/#3297 seam pattern). accept is the human's DEC-048 go — gated by
/// `can_accept` — so --atomic is the recognized standalone-invocation flag, not a
/// behavioral fork (the authority gate still enforces jeff/wren). Pure + testable:
/// run_accept feeds it env::args, so the CLI seam is covered, not just the lib fn.
pub fn parse_accept_args(args: &[String]) -> R<(u64, String, bool)> {
    let atomic = args.iter().any(|a| a == "--atomic");
    let pos: Vec<&String> = args.iter().filter(|a| a.as_str() != "--atomic").collect();
    let card_arg = pos
        .first()
        .ok_or_else(|| "usage: werk-accept <card> <role> [--atomic]".to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = pos
        .get(1)
        .map(|s| s.to_string())
        .ok_or_else(|| "usage: werk-accept <card> <role> [--atomic]".to_string())?;
    Ok((card, role, atomic))
}

/// ACCEPTER is $DEPLOY_ROLE (the authorizing identity, distinct from the builder
/// `role`). Writes Jeff's go to the demo witness; does NOT merge or finalize. Conforms
/// to ADR-032 (verb contract) + ADR-037 (--atomic): accept is the human's DEC-048 go,
/// so --atomic is the standalone door — recognized, authority still enforced by can_accept.
pub fn run_accept() -> R<String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let (card, role, _atomic) = parse_accept_args(&args)?;
    let accepter = env::var("DEPLOY_ROLE").unwrap_or_default();
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    run_accept_in(card, &role, &accepter, &home)
}

/// #3332 — the testable core of the #3311 fold: signal (authority-gated, records the
/// go) THEN finalize (mechanical close), in ONE call, joined by accept_output (silent
/// on the clean path, #3327). run_accept() is the thin env/arg wrapper; tests drive
/// run_accept_in directly against a fixture home (env::args() can't be set in-process).
/// Idempotent: a re-run after a partial completion (transport-drop / WIP-limbo) re-enters
/// here — signal hits the already-Done audit path, finalize is idempotent (cards-done on
/// a Done card no-ops), so the card converges to Done with no double-merge (finalize
/// never merges; werk-merge owns that, upstream in the pipeline).
pub fn run_accept_in(card: u64, role: &str, accepter: &str, home: &Path) -> R<String> {
    // #3311 — GO = ACCEPT: one exit verb. The authority-gated signal records who
    // accepted, then the mechanical close runs in the same invocation. werk-finalize
    // (the #3237 twin) is DELETED; this verb flips Done, closes branch + werk, stamps.
    let sig = signal(card, role, accepter, home)?;
    let fin = finalize(card, role, home)?;
    Ok(accept_output(&sig, &fin))
}

/// #3327 — GO is one silent ceremony. Join the signal + finalize messages for the
/// human: a CLEAN go (signal returns "" — recorded the go internally, no announce)
/// shows ONLY the finalize line, so the output never reads like a second go. The
/// already-Done audit path (signal returns a note) shows both. Pure → unit-tested.
pub fn accept_output(signal_msg: &str, finalize_msg: &str) -> String {
    if signal_msg.is_empty() {
        finalize_msg.to_string()
    } else {
        format!("{} | {}", signal_msg, finalize_msg)
    }
}


/// Shared FRONT for the decision verbs (werk-accept go, werk-do-more no-go|more):
/// DEC-048 authority (stage 1: only jeff/wren; stage 2: keyed on the card's REAL owner
/// so a self-accept can't be dodged by a different `role` arg) + resolve status. No
/// side effects beyond the refusal witness. Returns the card status on success so the
/// caller applies Done→idempotent / not-WIP→refuse. Keyed on DEPLOY_ROLE (accepter),
/// never the session role (#3086 live proof).
fn gate_decision(card: u64, role: &str, accepter: &str, home: &Path, trace: &str) -> R<String> {
    if !matches!(accepter, "jeff" | "wren") {
        jsonl(home, role, card, trace, "decision.refused", &fail_extra("unauthorized"));
        return Err(format!("accepter '{}' may not decide #{}: only jeff/wren (DEC-048)", accepter, card));
    }
    let cj = run(&script_path(home, "cards"), &["view", &card.to_string(), "--json"])
        .map_err(|e| format!("card #{} not viewable: {}", card, e))?;
    let owner = match json_str_field(&cj, "owner") {
        Some(o) => o.trim().to_lowercase(),
        None => {
            jsonl(home, role, card, trace, "decision.refused", &fail_extra("owner-unresolved"));
            return Err(format!("#{}: cannot resolve owner — refusing (can't prove non-self-accept)", card));
        }
    };
    if !can_accept(accepter, &owner) {
        jsonl(home, role, card, trace, "decision.refused", &fail_extra("self-accept"));
        return Err(format!(
            "accepter '{}' may not self-decide #{} (owner={}): no grading your own homework (DEC-048)",
            accepter, card, owner
        ));
    }
    Ok(json_str_field(&cj, "status").unwrap_or_default())
}

/// Append a demo.decision line to the witness werk-demo polls (ops/logs/werk-demo.jsonl).
/// Byte-exact via demo_decision_line so the comma-terminated card_id can't drift.
fn write_decision(home: &Path, card: u64, decision: &str, accepter: &str, trace: &str) {
    let p = home.join("ops/logs/werk-demo.jsonl");
    if let Some(d) = p.parent() { let _ = fs::create_dir_all(d); }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(demo_decision_line(ts, card, decision, accepter, trace).as_bytes());
    }
}

/// werk-accept (#3237) — Jeff's GO signal. Authority + WIP, then write demo.decision{go}
/// to the witness werk-demo blocks on. "go" IS the DEC-048 accept; this verb does NOT
/// merge and does NOT finalize (werk-merge / werk-finalize own those). Done → idempotent
/// no-op (don't re-write the decision); not-WIP → refuse.
pub fn signal(card: u64, role: &str, accepter: &str, home: &Path) -> R<String> {
    let accepter = accepter.trim().to_lowercase();
    let trace = trace_id();
    jsonl(home, role, card, &trace, "signal.started", &format!(",\"accepter\":\"{}\"", accepter));
    let status = gate_decision(card, role, &accepter, home, &trace)?;
    if status == "Done" {
        // #3298 — the land already finalized on the GO, which IS the DEC-048 accept. A
        // werk-accept AFTER that is NOT a no-op — it's the explicit post-finalize AUDIT:
        // record {who, when} so the human's attestation lands on the card thread, distinct
        // from the mechanical finalize. Resolves the land-vs-accept seam (the GO was the
        // accept; this is the recorded sign-off, not a "no decision to render").
        jsonl(home, role, card, &trace, "accept.audit",
            &format!(",\"accepter\":\"{}\",\"note\":\"post-finalize-attestation\"", accepter));
        return Ok(format!(
            "#{} already finalized — recorded {}'s post-finalize accept (audit, DEC-048). The GO was the accept.",
            card, accepter
        ));
    }
    if status != "WIP" {
        jsonl(home, role, card, &trace, "signal.refused", &format!("{},\"status\":\"{}\"", fail_extra("not-wip"), status));
        return Err(format!("#{} is '{}', not WIP — a go applies to a WIP card", card, status));
    }
    write_decision(home, card, "go", &accepter, &trace);
    jsonl(home, role, card, &trace, "signal.go", &format!(",\"accepter\":\"{}\"", accepter));
    // #3327 — GO is one silent ceremony. The go is RECORDED above (witness + signal.go
    // event, which finalize gates on); the human-facing announce is dropped. GO=accept
    // (#3311/DEC-048): the go happened at the demo, and finalize() runs in this same
    // invocation, so a "go signaled… act continues to merge" line here is a phantom
    // second-go for work already done. Return empty → run_accept shows finalize alone.
    Ok(String::new())
}


/// werk-finalize (#3237) — the MECHANICAL post-deploy finalize. NO authority gate (the
/// authority was the go); act runs this after merge+deploy-prod succeed. Idempotent: board Done,
/// card.accepted, teardown (env-down then chorus-werk remove), and chorus/accept on origin/main HEAD.
/// The HEAD sha is read from CANONICAL home (always present), NOT the werk, which this
/// verb removes — so the gh status posts regardless of teardown order.
///
/// #3499 — the demo.presented gate is GONE. finalize is a PURE STEP: it does its own
/// teardown work and never audits whether the demo ran. A downstream step checking an
/// upstream step's completion is the witness-wall anti-pattern (#3410's demo_presented
/// read failed to land cards whose demo was actually proven). Ordering is the
/// orchestrator's job (werk.yml runs demo → merge → accept, fail-stop): reaching
/// finalize MEANS demo + merge succeeded, structurally — not because this verb re-checks.
pub fn finalize(card: u64, role: &str, home: &Path) -> R<String> {
    let trace = trace_id();
    let home_s = path(home)?.to_string();
    jsonl(home, role, card, &trace, "accept.started", "");

    // serialize the board flip under one flock so concurrent finalizes can't race.
    let _lock = lock(home, Duration::from_secs(30))?;
    jsonl(home, role, card, &trace, "lock.acquired", "");

    run(&script_path(home, "cards"), &["done", &card.to_string()])
        .map_err(|e| format!("cards-done failed (re-run finalize to finish): {}", e))?;
    let _ = run(&script_path(home, "chorus-log"), &["card.accepted", role, &format!("card={}", card)]);
    jsonl(home, role, card, &trace, "card.done", "");

    // env-down BEFORE chorus-werk remove so variant services release handles into the
    // werk tree before it's deleted. Honest witness: emit the real teardown outcome.
    match run(&bin_path("werk-deploy"), &["env-down", role, &card.to_string()]) {
        Ok(_) => jsonl(home, role, card, &trace, "accept.env_down", ",\"result\":\"ok\""),
        Err(e) => jsonl(home, role, card, &trace, "accept.env_down.failed",
            &format!("{},\"result\":\"fail\",\"error\":\"{}\"", fail_extra("env-down-fail"), e.replace('"', "'"))),
    }

    // #3431: native teardown via werk-teardown (dirty-refuse, two-tier merge proof,
    // orphan-propagate, .werk-mcp teardown) — no chorus-werk shell-out. Stays
    // best-effort like the shell-out it replaces: post-accept teardown must never
    // block finalization, but the outcome is witnessed honestly either way.
    {
        let werk_base = env::var("CHORUS_WERK_BASE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.parent().map(|p| p.join("chorus-werk")).unwrap_or_else(|| home.join("..")));
        let mut emit = |event: &str, extras: &[(&str, &str)]| {
            let card_f = format!("card={}", card);
            let mut args: Vec<String> = vec![event.to_string(), role.to_string(), card_f];
            for (k, v) in extras {
                args.push(format!("{}={}", k, v));
            }
            let argrefs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            let _ = run(&script_path(home, "chorus-log"), &argrefs);
        };
        match werk_teardown::teardown_werk(home, &werk_base, role, card, &mut emit) {
            Ok(_) => jsonl(home, role, card, &trace, "accept.teardown", ",\"result\":\"ok\""),
            Err(e) => jsonl(home, role, card, &trace, "accept.teardown.failed",
                &format!("{},\"result\":\"fail\",\"error\":\"{}\"", fail_extra("teardown-fail"), e.to_string().replace('"', "'"))),
        }
    }

    // gh chorus/accept on the merged main HEAD — sha from CANONICAL (the werk may be gone
    // after remove; canonical always has origin/main). Kade's navigator call (#3).
    if let Ok(sha) = run_in(&home_s, "git", &["rev-parse", "origin/main"]).map(|s| s.trim().to_string()) {
        let _ = run_in(&home_s, "gh", &[
            "api", &format!("repos/{{owner}}/{{repo}}/statuses/{}", sha),
            "-f", "state=success",
            "-f", &format!("context=chorus/accept/{}", card),
            "-f", &format!("description=finalized trace={} status=accepted", trace),
        ]);
    }

    jsonl(home, role, card, &trace, "accept.completed", "");
    Ok(format!("#{} accepted (board Done + closed)", card))
}
