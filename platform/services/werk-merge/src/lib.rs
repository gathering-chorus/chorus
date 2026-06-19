//! werk-merge — `/werk/merge` v2 logic (atomic PR open+merge verb, #3175).
//!
//! Self-contained: std only + a direct libc `flock` extern; calls `git` and `gh`
//! as subprocesses. No dependency on any other chorus code. The v2 verb-binary
//! blueprint is werk-pull (#3045).
//!
//! ATOMIC VERB — merge ONLY. Lands an already-pushed card branch `<role>/<card>`
//! to `main` and content-verifies the merge actually happened. It RETIRES two
//! inline `gh pr merge <branch>` call sites that resolved a PR by branch NAME:
//!   - werk-accept's finalize block (accept becomes finalize-only)
//!   - werk-mcp.sh step 5's interim `gh pr merge`
//!
//! Both were `merged≠live` generators (2026-06-03, Wren + Kade): a stale, already-merged PR with the same branch name matched, gh reported "already merged", and ZERO new commits landed — caught only by content-verify.
//!
//! THE FIX (#3175):
//!   - Resolve the OPEN PR for the current HEAD oid, not the branch name.
//!   - Create the PR if none open for this oid (covers never-PR'd + stale-merged).
//!   - ONE merge mechanism: squash (the 2026-06-02 Kade+Silas decision — binary = f(source TREE), so squash vs --merge is irrelevant to the artifact).
//!   - POST-MERGE CONTENT-VERIFY: the PR is MERGED and its merge commit is on origin/main. Never assumed — verified, or the verb refuses.
//!   - Typed R<T> refusals: no-werk / branch-mismatch / no-open-pr / pr-create-fail / merge-conflict / not-mergeable / merge-fail.
//!   - All-or-nothing under one flock; JSONL witness per step (never affects the op); idempotent (this exact oid already merged onto main -> no-op success).

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// #3513 — the ONE shared failure classifier (failure_class / fail_extra), lifted
// out of this crate's private #3495 copy so every verb classifies identically.
include!("../../shared/failure_class.rs");

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}
const LOCK_EX_NB: i32 = 0x02 | 0x04; // LOCK_EX | LOCK_NB
const LOCK_UN: i32 = 0x08;

pub type R<T> = Result<T, String>;

// ─────────────────────────── pure helpers (unit-tested) ───────────────────────────

pub fn branch_name(role: &str, card: u64) -> String {
    format!("{}/{}", role, card)
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

/// Find a `"key":"value"` string field within a flat JSON object slice. Tolerant of
/// whitespace around the colon. Zero-dep — gh's `--json` output for our narrow
/// queries (number, headRefOid, state, oid) is flat and well-formed.
fn json_str_field(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let i = s.find(&pat)? + pat.len();
    let rest = &s[i..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

/// Find a `"key":<int>` numeric field within a flat JSON object slice.
fn json_int_field(s: &str, key: &str) -> Option<u64> {
    let pat = format!("\"{}\"", key);
    let i = s.find(&pat)? + pat.len();
    let rest = &s[i..];
    let colon = rest.find(':')?;
    let digits: String = rest[colon + 1..].trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

/// Split a flat JSON array `[{...},{...}]` into its top-level object slices.
fn split_objects(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in json.chars() {
        match ch {
            '{' => {
                depth += 1;
                cur.push(ch);
            }
            '}' => {
                depth -= 1;
                cur.push(ch);
                if depth == 0 {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ if depth > 0 => cur.push(ch),
            _ => {}
        }
    }
    out
}

/// KEYSTONE (#3175): given `gh pr list --json number,headRefOid` output, return the
/// PR number whose headRefOid == the current HEAD sha. Resolve by OID, never by
/// branch name — the whole point of the card. None if no object matches.
pub fn pr_number_for_sha(json: &str, head_sha: &str) -> Option<u64> {
    for obj in split_objects(json) {
        if json_str_field(&obj, "headRefOid").as_deref() == Some(head_sha) {
            return json_int_field(&obj, "number");
        }
    }
    None
}

/// Map a failed `gh pr merge` stderr to a typed refusal reason.
pub fn classify_merge_error(stderr: &str) -> &'static str {
    let s = stderr.to_lowercase();
    if s.contains("conflict") {
        "merge-conflict"
    } else if s.contains("not mergeable") || s.contains("not in mergeable") || s.contains("mergeable state") {
        "not-mergeable"
    } else {
        "merge-fail"
    }
}

// #3513 — failure_class + refused_extra (renamed fail_extra) were lifted to the
// shared classifier at services/shared/failure_class.rs (include!d above). One
// classifier, all verbs. merge.refused now calls the shared `fail_extra`.

// #3499 — REMOVED: round_check / RoundCheck / demoed_patch_ids / round_landable /
// patch_id. These all served the demo-facts land gate (#3365/#3459/#3461 round +
// patch-id reconciliation, and the later announce_ready_full facts check). The
// gate is gone — werk-merge no longer audits the demo step (orchestrator owns
// ordering, #3499) — so the scaffolding that fed it is gone with it. #3495's
// failure_class / refused_extra stay: they label REAL refusals (no-werk,
// pr-create-fail, merge-fail, …) with the DORA discriminator, independent of the
// retired demo gate.

// ─────────────────────────── side-effecting helpers ───────────────────────────

fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-merge.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let line = jsonl_line(ts, event, role, card, trace, extra);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Run a CLI in a working dir; non-zero exit is a typed error carrying stderr.
fn run_in(dir: &str, cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

pub fn lock(home: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = home.join(".git/chorus-merge.lock");
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false) // a lock file is opened only for flock — never truncate it.
        .open(&p)
        .map_err(|e| format!("cannot open lock {}: {}", p.display(), e))?;
    let start = Instant::now();
    loop {
        if unsafe { flock(f.as_raw_fd(), LOCK_EX_NB) } == 0 {
            return Ok(FlockGuard(f));
        }
        if start.elapsed() >= timeout {
            return Err("another merge holds the repo lock (timed out after 30s)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// True iff `oid` is present on origin/main (i.e., the merge commit landed).
fn oid_on_main(werk_s: &str, oid: &str) -> bool {
    // `git merge-base --is-ancestor <oid> origin/main` exits 0 iff oid is reachable.
    Command::new("git")
        .args(["merge-base", "--is-ancestor", oid, "origin/main"])
        .current_dir(werk_s)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Resolve the OPEN PR for `head_sha`, or None. (gh pr list filtered to open + branch;
/// we re-filter by oid in Rust so the by-oid contract is ours, not jq's.)
fn open_pr_for_sha(werk_s: &str, branch: &str, head_sha: &str) -> R<Option<u64>> {
    let json = run_in(
        werk_s,
        "gh",
        &["pr", "list", "--state", "open", "--head", branch, "--json", "number,headRefOid"],
    )?;
    Ok(pr_number_for_sha(&json, head_sha))
}

/// #3336 — content-verify idempotency decision. `git diff --quiet origin/main HEAD`
/// exits 0 (-> diff_quiet_ok=true) iff the two trees are IDENTICAL, i.e. HEAD's content
/// is already on origin/main. This is the squash/branch-deletion-robust "did this land?"
/// signal the by-oid PR match can't give. Pure so the exit-code→meaning mapping is pinned
/// by test (a future refactor that inverts the sense fails loudly).
pub fn head_content_merged(diff_quiet_ok: bool) -> bool {
    diff_quiet_ok
}

/// Resolve a MERGED PR for `head_sha` (idempotency: this exact work already landed).
fn merged_pr_for_sha(werk_s: &str, branch: &str, head_sha: &str) -> Option<u64> {
    run_in(
        werk_s,
        "gh",
        &["pr", "list", "--state", "merged", "--head", branch, "--json", "number,headRefOid"],
    )
    .ok()
    .and_then(|json| pr_number_for_sha(&json, head_sha))
}

// ─────────────────────────── the verb ───────────────────────────

/// Entry: parse the contract args (`werk-merge <card> <role>`, role falls back to
/// #3297 — parse contract args + recognize `--atomic` ANYWHERE (push-pattern seam;
/// #3296/#3306 lesson: cover the CLI argv parse, not just the lib fn).
pub fn parse_merge_args(args: &[String], deploy_role: Option<String>) -> R<(u64, String, bool)> {
    let atomic = args.iter().any(|a| a == "--atomic");
    let pos: Vec<&String> = args.iter().filter(|a| a.as_str() != "--atomic").collect();
    let card_arg = pos
        .first()
        .ok_or_else(|| "usage: werk-merge <card> <role> [--atomic]".to_string())?;
    let card: u64 = card_arg
        .parse()
        .map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = pos
        .get(1)
        .map(|s| s.to_string())
        .or(deploy_role)
        .ok_or_else(|| "usage: werk-merge <card> <role> [--atomic] (or set DEPLOY_ROLE)".to_string())?;
    Ok((card, role, atomic))
}

/// #3297 — the ADR-037 approval gate (merge mutates main; one gate, two doors). FLOW:
/// the demo→GO is the approval (the land supplies the accepter via $ACCEPTER) — record,
/// don't gate. `--atomic` (standalone, no land-GO): the verb DEMANDS an accepter or
/// refuses — the door `--atomic` must never become a quiet unauthorized ship. Returns
/// who authorized (for the {who, what, when} spine event).
pub fn require_approval(atomic: bool, accepter: Option<String>) -> R<String> {
    match accepter {
        Some(a) if !a.trim().is_empty() => Ok(a),
        _ if atomic => Err(
            "no-approval: merge --atomic mutates main — set ACCEPTER=<who> to authorize this standalone merge"
                .to_string(),
        ),
        _ => Ok("flow".to_string()), // flow: the demo→GO was the approval, recorded at the land
    }
}

/// chorus-log args contract (mirrors werk-commit): `event role card=N trace=T k=v...`.
pub fn spine_args(event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) -> Vec<String> {
    let mut v = vec![
        event.to_string(),
        role.to_string(),
        format!("card={}", card),
        format!("trace={}", trace),
    ];
    for (k, val) in extras {
        v.push(format!("{}={}", k, val));
    }
    v
}

/// Best-effort spine emit (#3297 — the approval event must reach the ONE spine, not just
/// the jsonl witness, so {who, what, when} is queryable; ADR-037 "kills invisible").
fn emit_spine(home: &Path, event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) {
    let log = home.join("platform/scripts/chorus-log");
    let _ = Command::new(&log)
        .args(spine_args(event, role, card, trace, extras))
        .output();
}

/// Entry: parse the contract args (`werk-merge <card> <role> [--atomic]`, role falls
/// back to $DEPLOY_ROLE) + env, then run the verb. The accepter (who authorized) comes
/// from $ACCEPTER — the land sets it; `--atomic` operators set it explicitly.
pub fn run_merge() -> R<String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let (card, role, atomic) = parse_merge_args(&args, env::var("DEPLOY_ROLE").ok())?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    let accepter = env::var("ACCEPTER").ok().filter(|s| !s.trim().is_empty());
    merge_inner(card, &role, &home, &werk_base, atomic, accepter)
}

/// The whole verb, all inputs explicit so it is testable against a real temp repo
/// (deps injected via PATH: real `git`, stateful shimmed `gh`). Returns the merged
/// origin/main sha.
/// Flow entry (atomic=false): the land's demo→GO is the approval; accepter from $ACCEPTER.
pub fn merge(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let accepter = env::var("ACCEPTER").ok().filter(|s| !s.trim().is_empty());
    merge_inner(card, role, home, werk_base, false, accepter)
}

/// #3297 — `merge --atomic`: standalone PR squash-merge (the deadlock hand-recovery).
/// Requires an explicit accepter (ADR-037 approval gate — merge mutates main).
pub fn merge_atomic(card: u64, role: &str, home: &Path, werk_base: &Path, accepter: Option<String>) -> R<String> {
    merge_inner(card, role, home, werk_base, true, accepter)
}

/// The whole verb, all inputs explicit so it is testable against a real temp repo.
/// `atomic` selects the standalone door; `accepter` is who authorized (ADR-037).
fn merge_inner(card: u64, role: &str, home: &Path, werk_base: &Path, atomic: bool, accepter: Option<String>) -> R<String> {
    let trace = trace_id();
    // #3297 — approval gate up front (ADR-037): --atomic demands an accepter or refuses;
    // in the flow the demo→GO already authorized, so this returns "flow".
    let approver = require_approval(atomic, accepter)?;
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "merge.started", "");

    // Refuse if no werk (deterministic, no canonical fallback — the #3012/#3013 fix).
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "merge.refused", &fail_extra("no-werk"));
        return Err(format!("no werk for #{} at {} — pull/commit/push the card first", card, werk.display()));
    }
    // werk must be on the card's branch (carries the #2580 cross-role intent).
    let cur = run_in(&werk_s, "git", &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    if cur != branch {
        jsonl(home, role, card, &trace, "merge.refused", &fail_extra("branch-mismatch"));
        return Err(format!("werk is on '{}', not '{}' — refusing to merge", cur, branch));
    }

    let head_sha = run_in(&werk_s, "git", &["rev-parse", "HEAD"])?.trim().to_string();
    // Refresh remote view so PR resolution + content-verify see current origin/main.
    let _ = run_in(&werk_s, "git", &["fetch", "-q", "origin", "main"]);

    // Idempotent: this exact oid already merged onto main -> no-op success.
    if let Some(pr) = merged_pr_for_sha(&werk_s, &branch, &head_sha) {
        let main_sha = run_in(&werk_s, "git", &["rev-parse", "origin/main"]).unwrap_or_default().trim().to_string();
        jsonl(home, role, card, &trace, "merge.idempotent", &format!(",\"pr\":{},\"sha\":\"{}\"", pr, main_sha));
        return Ok(main_sha);
    }

    // #3336 — CONTENT-VERIFY idempotency (the dropped-land-resume fix). The by-oid check
    // above is brittle: a SQUASH merge orphans the branch tip's oid, and GitHub may delete
    // the branch on merge, so `gh pr list --head <branch>` + headRefOid match can miss a
    // land that genuinely happened. The robust signal is CONTENT: if HEAD's tree is already
    // identical to origin/main (`git diff --quiet` exits 0 = no diff), the work is on main —
    // no-op success regardless of PR/branch/oid state. This makes a transport-dropped land
    // resume cleanly (re-run finalizes) instead of a false no-open-pr stranding it in WIP.
    // `git diff --quiet A B` exits 0 (Ok) when the trees are identical, 1 (Err) when they
    // differ — so is_ok() == "HEAD's content is already on origin/main". head_content_merged
    // encodes that semantic as a pure, testable boolean.
    let diff_quiet_ok = run_in(&werk_s, "git", &["diff", "--quiet", "origin/main", "HEAD"]).is_ok();
    if head_content_merged(diff_quiet_ok) {
        let main_sha = run_in(&werk_s, "git", &["rev-parse", "origin/main"]).unwrap_or_default().trim().to_string();
        jsonl(home, role, card, &trace, "merge.idempotent",
            &format!(",\"reason\":\"content-on-main\",\"sha\":\"{}\"", main_sha));
        return Ok(main_sha);
    }

    // #3476 — PUSH local HEAD to origin BEFORE the by-oid PR-resolve. The #3175
    // header assumed an already-pushed branch, but a post-present werk-commit
    // advances LOCAL HEAD without pushing — so origin's branch sits at the OLD oid,
    // gh pr list returns the open PR at that stale headRefOid, the by-oid match
    // misses, and the create-fallback hits "a pull request for this branch already
    // exists" → pr-create-fail. (Jeff: "happens almost every cw"; Wren root-caused
    // it on #3466.) Pushing first makes origin == local, so the open PR's
    // headRefOid == HEAD and the resolve finds it — no spurious create. --force is
    // safe: it's the role's own per-card branch (one werk, one writer), and it
    // covers the rebase-onto-advancing-main (round-churn) case where a plain push
    // would be non-fast-forward. Best-effort + witnessed: a push failure degrades to
    // the existing resolve/create path (no regression), never strands silently.
    match run_in(&werk_s, "git", &["push", "--force", "origin", &format!("HEAD:{}", branch)]) {
        Ok(_) => jsonl(home, role, card, &trace, "merge.branch.pushed",
            &format!(",\"sha\":\"{}\",\"branch\":\"{}\"", head_sha, branch)),
        Err(e) => jsonl(home, role, card, &trace, "merge.branch.push.failed",
            &format!("{},\"branch\":\"{}\",\"err\":\"{}\"", fail_extra("push-rejected"), branch, e.replace('"', "'"))),
    }

    // Resolve the OPEN PR for the current HEAD oid — NOT the branch name. Create one
    // if none open for this oid (covers never-PR'd AND the stale-merged-PR case that
    // generated the false-green). This is the #3175 fix.
    let pr = match open_pr_for_sha(&werk_s, &branch, &head_sha)? {
        Some(n) => n,
        None => {
            jsonl(home, role, card, &trace, "merge.pr.create", &format!(",\"sha\":\"{}\"", head_sha));
            run_in(
                &werk_s,
                "gh",
                &[
                    "pr", "create", "--base", "main", "--head", &branch,
                    "--title", &format!("#{} ({})", card, role),
                    "--body", &format!("werk-merge #{} — role={} trace={} sha={}", card, role, trace, head_sha),
                ],
            )
            .map_err(|e| {
                jsonl(home, role, card, &trace, "merge.refused", &fail_extra("pr-create-fail"));
                format!("pr-create-fail: {}", e)
            })?;
            // re-resolve by oid. GitHub's list API lags its create API — tonight's
            // #3269 land hit exactly this (create succeeded, immediate re-list missed
            // it, land went red while the PR sat open). A bounded backoff kills that
            // false-red class (#3266 AC1); a PR still invisible after ~14s is real.
            let mut found = None;
            for (attempt, wait_s) in [0u64, 2, 4, 8].iter().enumerate() {
                if *wait_s > 0 {
                    std::thread::sleep(Duration::from_secs(*wait_s));
                    jsonl(home, role, card, &trace, "merge.pr.resolve.retry",
                        &format!(",\"attempt\":{}", attempt));
                }
                found = open_pr_for_sha(&werk_s, &branch, &head_sha)?;
                if found.is_some() {
                    break;
                }
            }
            found.ok_or_else(|| {
                jsonl(home, role, card, &trace, "merge.refused", &fail_extra("no-open-pr"));
                // #3336 — if you hit this on a MANUAL resume, the usual cause is operator
                // discipline, not a bug: the local HEAD commit was not PUSHED before go, so
                // no PR matches the oid (and the content-verify above found nothing on main
                // either, because the unpushed work never merged). PUSH first, then resume.
                "no-open-pr: created a PR but none open matches HEAD oid (after 4 resolve attempts over ~14s). \
                 If resuming a manual land: push HEAD first (the content isn't on main and no PR matches the oid)."
                    .to_string()
            })?
        }
    };

    // #3499 — the demo-facts gate is GONE from this verb. werk-merge is a PURE
    // STEP: it does its own work (resolve the PR by HEAD oid, squash-merge,
    // content-verify) and NEVER audits whether the demo step ran. A downstream
    // step checking an upstream step's completion is the anti-pattern that built
    // the witness wall — merge grepping werk-demo.jsonl for a `demo.presented`
    // event that failed to emit for a dozen independent reasons broke every land
    // 2026-06-18/19 while the work was actually proven.
    //
    // Ordering is the ORCHESTRATOR's job (werk.yml): it runs `demo` and only on
    // the human GO runs `merge` (the go-invocation is the only-door). Reaching
    // merge MEANS the demo presented and Jeff gave go — structurally, not because
    // the verb re-derives it. The override and the per-round demo check are gone,
    // not relocated (that check WAS the churn). (Retired with this block: the
    // #3365/#3459/#3461 round_check / patch-id / demo.presented scaffolding.)

    // #3297 — record the approval on the spine BEFORE the irreversible merge (ADR-037
    // {who, what, when} — kills "invisible"). Flow approvals carry accepter="flow".
    jsonl(home, role, card, &trace, "merge.approved",
        &format!(",\"accepter\":\"{}\",\"pr\":{},\"atomic\":{}", approver, pr, atomic));
    emit_spine(home, "merge.approved", role, card, &trace,
        &[("accepter", &approver), ("pr", &pr.to_string()), ("atomic", &atomic.to_string())]);

    // ── merge, serialized under one flock. ONE mechanism: squash. ──
    {
        let _lock = lock(home, Duration::from_secs(30))?;
        jsonl(home, role, card, &trace, "lock.acquired", &format!(",\"pr\":{}", pr));
        if let Err(e) = run_in(&werk_s, "gh", &["pr", "merge", &pr.to_string(), "--squash"]) {
            let reason = classify_merge_error(&e);
            jsonl(home, role, card, &trace, "merge.refused", &fail_extra(reason));
            return Err(format!("{}: pr #{} did not merge; nothing landed: {}", reason, pr, e));
        }
        jsonl(home, role, card, &trace, "merged", &format!(",\"pr\":{}", pr));
    }

    // ── POST-MERGE CONTENT-VERIFY (#3175): the PR is MERGED and its merge commit is
    // on origin/main. Never assumed. This is the guard that catches the false-green:
    // gh reporting success while ZERO commits landed. ──
    let _ = run_in(&werk_s, "git", &["fetch", "-q", "origin", "main"]);
    let view = run_in(&werk_s, "gh", &["pr", "view", &pr.to_string(), "--json", "state,mergeCommit"])
        .map_err(|e| format!("merge-fail: cannot verify pr #{} state: {}", pr, e))?;
    let state = json_str_field(&view, "state").unwrap_or_default();
    let merge_oid = json_str_field(&view, "oid").unwrap_or_default();
    if state != "MERGED" {
        jsonl(home, role, card, &trace, "merge.refused", &fail_extra("merge-fail"));
        return Err(format!("merge-fail: pr #{} state is '{}', not MERGED — content did not land", pr, state));
    }
    if merge_oid.is_empty() || !oid_on_main(&werk_s, &merge_oid) {
        jsonl(home, role, card, &trace, "merge.refused", &fail_extra("merge-fail"));
        return Err(format!(
            "merge-fail: pr #{} reports MERGED but merge commit {} is not on origin/main (false-green guard)",
            pr, merge_oid
        ));
    }
    jsonl(home, role, card, &trace, "content.verified", &format!(",\"merge_commit\":\"{}\"", merge_oid));

    let main_sha = run_in(&werk_s, "git", &["rev-parse", "origin/main"]).unwrap_or_default().trim().to_string();

    // gh chorus/merge/<card> status on the merged main HEAD (best-effort, like push).
    let endpoint = format!("repos/{{owner}}/{{repo}}/statuses/{}", main_sha);
    let desc = format!("role={} trace={} pr={} sha={} status=merged", role, trace, pr, main_sha);
    let _ = run_in(
        &werk_s,
        "gh",
        &[
            "api", &endpoint,
            "-f", "state=success",
            "-f", &format!("context=chorus/merge/{}", card),
            "-f", &format!("description={}", desc),
        ],
    );

    jsonl(home, role, card, &trace, "merge.completed", &format!(",\"pr\":{},\"sha\":\"{}\"", pr, main_sha));
    Ok(main_sha)
}
