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

/// Entry: parse args (`werk-accept <card> <role>`; the ACCEPTER is read from
/// $DEPLOY_ROLE — that is who is authorizing the finalize, distinct from the
/// builder `role` whose card is being accepted) + env, then run the verb.
pub fn run_accept() -> R<String> {
    let card_arg = env::args().nth(1).ok_or_else(|| "usage: werk-accept <card> <role>".to_string())?;
    let card: u64 = card_arg.parse().map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = env::args().nth(2).ok_or_else(|| "usage: werk-accept <card> <role>".to_string())?;
    // the accepter is the authorizing identity, NOT the builder. Defaults to jeff
    // only when DEPLOY_ROLE says so — there is no implicit self-accept.
    let accepter = env::var("DEPLOY_ROLE").unwrap_or_default();
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base = PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    accept(card, &role, &accepter, &home, &werk_base)
}

/// The whole verb, all inputs explicit so it is testable against a real temp repo
/// (deps injected via PATH: real `git`, shimmed `gh`/`cards`/`chorus-log`/`chorus-werk`).
pub fn accept(card: u64, role: &str, accepter: &str, home: &Path, werk_base: &Path) -> R<String> {
    // normalize the accepter (trim + lowercase) so a mis-cased "Jeff" / " wren "
    // hits its real authority row instead of silently dropping to fail-closed REFUSE.
    let accepter = accepter.trim().to_lowercase();
    let trace = trace_id();
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "accept.started", &format!(",\"accepter\":\"{}\"", accepter));

    // AUTHORITY GATE (DEC-048) stage 1: only jeff/wren are accept authorities —
    // refuse kade/silas before touching anything. The self-accept nuance is stage 2,
    // checked against the card's REAL owner after the board query (below) so it can't
    // be dodged by passing a different `role` arg. Keyed on DEPLOY_ROLE (accepter),
    // never the session role — distinguishes "Jeff accepting in Wren's session" from
    // "Wren self-accepting" (Wren's #3086 live proof).
    if !matches!(accepter.as_str(), "jeff" | "wren") {
        jsonl(home, role, card, &trace, "accept.refused", ",\"reason\":\"unauthorized\"");
        return Err(format!(
            "accepter '{}' may not finalize #{}: only jeff/wren accept (DEC-048)", accepter, card
        ));
    }

    // #3012/#3013 fix: deterministic werk, REFUSE if absent. No canonical fallback.
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "accept.refused", ",\"reason\":\"no-werk\"");
        return Err(format!("no werk for #{} at {} — nothing to accept", card, werk.display()));
    }
    let cur = run_in(&werk_s, "git", &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    if cur != branch {
        jsonl(home, role, card, &trace, "accept.refused", ",\"reason\":\"wrong-branch\"");
        return Err(format!("werk is on '{}', not '{}' — refusing to accept", cur, branch));
    }

    // EXIT precondition (not entry re-gate): the card must be in WIP. Idempotent if
    // already Done (a prior accept finalized it) -> no-op success.
    let cj = run(&script_path(home, "cards"), &["view", &card.to_string(), "--json"])
        .map_err(|e| format!("card #{} not viewable: {}", card, e))?;
    let status = json_str_field(&cj, "status").unwrap_or_default();

    // AUTHORITY GATE stage 2: keyed on the card's REAL owner (normalized), not the
    // role arg. Fail CLOSED — if the owner can't be resolved we cannot prove this
    // isn't a self-accept, so refuse. This is where DEPLOY_ROLE=wren accepting her
    // OWN card is blocked (#2979/DEC-048 — no exception for the accepter-in-chief).
    let owner = match json_str_field(&cj, "owner") {
        Some(o) => o.trim().to_lowercase(),
        None => {
            jsonl(home, role, card, &trace, "accept.refused", ",\"reason\":\"owner-unresolved\"");
            return Err(format!("#{}: cannot resolve card owner — refusing (can't prove non-self-accept)", card));
        }
    };
    if !can_accept(&accepter, &owner) {
        jsonl(home, role, card, &trace, "accept.refused", ",\"reason\":\"self-accept\"");
        return Err(format!(
            "accepter '{}' may not self-accept #{} (owner={}): no grading your own homework (DEC-048)",
            accepter, card, owner
        ));
    }

    if status == "Done" {
        jsonl(home, role, card, &trace, "accept.idempotent", "");
        return Ok(format!("#{} already accepted", card));
    }
    if status != "WIP" {
        jsonl(home, role, card, &trace, "accept.refused", ",\"reason\":\"not-wip\"");
        return Err(format!("#{} is '{}', not WIP — accept finalizes WIP cards only", card, status));
    }

    // branch must already be on origin (push is the upstream verb).
    if run_in(&werk_s, "git", &["ls-remote", "origin", &branch]).unwrap_or_default().trim().is_empty() {
        jsonl(home, role, card, &trace, "accept.refused", ",\"reason\":\"not-pushed\"");
        return Err(format!("#{}: branch {} not on origin — run push before accept", card, branch));
    }

    // --- finalize, serialized under one flock. NO git merge here (#3175): werk-merge
    // is the ONE merge mechanism and runs earlier in the sequence (…→merge→accept).
    // accept is FINALIZE-ONLY — board Done + spine + close. The lock serializes the
    // board flip so concurrent finalizes can't race. ---
    let _lock = lock(home, Duration::from_secs(30))?;
    jsonl(home, role, card, &trace, "lock.acquired", "");

    // finalize (idempotent on re-run): board Done + spine + close + gh.
    run(&script_path(home, "cards"), &["done", &card.to_string()])
        .map_err(|e| format!("cards-done failed (re-run accept to finish): {}", e))?;
    let _ = run(&script_path(home, "chorus-log"), &["card.accepted", role, &format!("card={}", card)]);
    jsonl(home, role, card, &trace, "card.done", "");

    // #3108 AC#8: tear down the per-card variant overlay BEFORE removing the
    // werk. env-down boots out the LaunchAgents + removes plists + marker
    // files for the role's variant services (per-role api/mcp ports). Doing
    // this first lets the services release any handles into the werk tree
    // before chorus-werk-remove deletes that tree. Best-effort + idempotent:
    // env-down is a no-op when no overlay is active, matching the existing
    // "let _ = run(...)" pattern for accept's finalize steps.
    // #3215: pass the CARD so env-down's spine events (env.down.started/stopped/
    // completed) thread the card's trace — Borg pairs them against the demo's
    // env.up.smoked{card}. And make the witness HONEST: the old line jsonl'd
    // `accept.env_down` UNCONDITIONALLY, claiming teardown even when it failed
    // (the dark-lifecycle root — a teardown that lied while daemons stayed up).
    // Emit the real outcome; a failed teardown is a leak to SEE, not a silent
    // success. Still non-fatal — the card IS accepted; a leaked daemon is an
    // ops gap to surface, not a reason to un-accept. Resolves with #3183:
    // keep the absolute bin_path("werk-deploy") resolution (never bare PATH).
    match run(&bin_path("werk-deploy"), &["env-down", role, &card.to_string()]) {
        Ok(_) => jsonl(home, role, card, &trace, "accept.env_down", ",\"result\":\"ok\""),
        Err(e) => jsonl(home, role, card, &trace, "accept.env_down.failed",
            &format!(",\"result\":\"fail\",\"error\":\"{}\"", e.replace('"', "'"))),
    }

    // close branch + werk (idempotent).
    let _ = run(&script_path(home, "chorus-werk"), &["remove", role, &card.to_string()]);

    // gh chorus/accept status on the merged main HEAD.
    if let Ok(sha) = run_in(&werk_s, "git", &["rev-parse", "origin/main"]).map(|s| s.trim().to_string()) {
        let _ = run_in(&werk_s, "gh", &[
            "api", &format!("repos/{{owner}}/{{repo}}/statuses/{}", sha),
            "-f", "state=success",
            "-f", &format!("context=chorus/accept/{}", card),
            "-f", &format!("description=accepter={} trace={} status=accepted", accepter, trace),
        ]);
    }

    jsonl(home, role, card, &trace, "accept.completed", "");
    Ok(format!("#{} accepted by {}", card, accepter))
}
