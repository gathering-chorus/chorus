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

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}
const LOCK_EX_NB: i32 = 0x02 | 0x04;
const LOCK_UN: i32 = 0x08;

pub type R<T> = Result<T, String>;

// --- pure helpers (unit-tested) ---

pub fn branch_name(role: &str, card: u64) -> String {
    format!("{}/{}", role, card)
}

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

/// #3116 — true iff the proving ceremony recorded a passing verdict for this
/// card: a `demo.verdict` event with `"verdict":"pass"` in the werk-demo witness
/// (ops/logs/werk-demo.jsonl, written by the demo binary). accept() gates on the
/// demo's OWN record, replacing the demo:preflight-pass comment + the retired
/// 4-event demo.*.completed chain. Missing witness = no demo ran = false. The
/// card_id match is comma-terminated so #3116 can't collide with #31160.
pub fn demo_verdict_pass(home: &Path, card: u64) -> bool {
    let witness = home.join("ops/logs/werk-demo.jsonl");
    match std::fs::read_to_string(&witness) {
        Ok(text) => {
            let card_key = format!("\"card_id\":{},", card);
            text.lines().any(|l| {
                l.contains("\"event\":\"demo.verdict\"")
                    && l.contains(&card_key)
                    && l.contains("\"verdict\":\"pass\"")
            })
        }
        Err(_) => false,
    }
}

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
    // #3311 — GO = ACCEPT: one exit verb. The authority-gated signal records who
    // accepted, then the mechanical close runs in the same invocation (idempotent —
    // a re-run completes a partial finalize). werk-finalize (the #3237 twin that held
    // the close while accept held only the signal) is DELETED; this verb now does
    // what its own header always claimed: flip Done, close branch + werk, stamp.
    let sig = signal(card, &role, &accepter, &home)?;
    let fin = finalize(card, &role, &home)?;
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
        jsonl(home, role, card, trace, "decision.refused", ",\"reason\":\"unauthorized\"");
        return Err(format!("accepter '{}' may not decide #{}: only jeff/wren (DEC-048)", accepter, card));
    }
    let cj = run(&script_path(home, "cards"), &["view", &card.to_string(), "--json"])
        .map_err(|e| format!("card #{} not viewable: {}", card, e))?;
    let owner = match json_str_field(&cj, "owner") {
        Some(o) => o.trim().to_lowercase(),
        None => {
            jsonl(home, role, card, trace, "decision.refused", ",\"reason\":\"owner-unresolved\"");
            return Err(format!("#{}: cannot resolve owner — refusing (can't prove non-self-accept)", card));
        }
    };
    if !can_accept(accepter, &owner) {
        jsonl(home, role, card, trace, "decision.refused", ",\"reason\":\"self-accept\"");
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
        jsonl(home, role, card, &trace, "signal.refused", &format!(",\"reason\":\"not-wip\",\"status\":\"{}\"", status));
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
/// authority was the go); act runs this after merge+deploy-prod succeed. Gated only on
/// the demo recording verdict=pass (the go path). Idempotent: board Done, card.accepted,
/// teardown (env-down then chorus-werk remove), and chorus/accept on origin/main HEAD.
/// The HEAD sha is read from CANONICAL home (always present), NOT the werk, which this
/// verb removes — so the gh status posts regardless of teardown order.
pub fn finalize(card: u64, role: &str, home: &Path) -> R<String> {
    let trace = trace_id();
    let home_s = path(home)?.to_string();
    jsonl(home, role, card, &trace, "finalize.started", "");

    if !demo_verdict_pass(home, card) {
        jsonl(home, role, card, &trace, "finalize.refused", ",\"reason\":\"no-demo-verdict\"");
        return Err(format!(
            "#{}: no demo.verdict=pass on record — werk-demo records it on go (#3116)", card
        ));
    }

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
        Ok(_) => jsonl(home, role, card, &trace, "finalize.env_down", ",\"result\":\"ok\""),
        Err(e) => jsonl(home, role, card, &trace, "finalize.env_down.failed",
            &format!(",\"result\":\"fail\",\"error\":\"{}\"", e.replace('"', "'"))),
    }

    let _ = run(&script_path(home, "chorus-werk"), &["remove", role, &card.to_string()]);

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

    jsonl(home, role, card, &trace, "finalize.completed", "");
    Ok(format!("#{} finalized (board Done + closed)", card))
}
