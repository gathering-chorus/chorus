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

/// #3459 — the #3365 round-validity decision.
///
/// The announce gate must let a content-preserving rebase merge (the demoed CHANGE
/// is unchanged, only the base moved when a peer landed) while still refusing
/// genuinely un-demoed content. The stable invariant is the card's DIFF — a
/// `git patch-id` of `merge-base..HEAD` — NOT the commit-sha (churns on every
/// rebase, the bug this fixes) and NOT the tree (= base+diff, so it churns when the
/// base moves under a rebase). git itself uses patch-id to detect already-applied
/// patches; identical change ⇒ identical patch-id, across any clean rebase.
#[derive(Debug, PartialEq, Eq)]
pub enum RoundCheck {
    /// The demo.presented witness carried this exact head-sha round — proceed (common case).
    ExactSha,
    /// Sha moved, but the current diff's patch-id matches a demoed one — a rebase of
    /// the SAME change that was demoed. Proceed; witness `round-rebased`.
    RebasedSameChange,
    /// No exact sha match and no matching patch-id — genuinely un-demoed content. Refuse.
    NoMatch,
}

/// Pure decision. `exact_sha` = a demo.presented witness carried this exact head-sha.
/// `current_patch` = patch-id of the current `merge-base..HEAD` diff (None if it
/// couldn't be computed — e.g. an empty diff). `demoed_patches` = patch-ids recorded
/// on this card's demo.presented witnesses.
pub fn round_check(exact_sha: bool, current_patch: Option<&str>, demoed_patches: &[String]) -> RoundCheck {
    if exact_sha {
        return RoundCheck::ExactSha;
    }
    match current_patch {
        Some(cur) if !cur.is_empty() && demoed_patches.iter().any(|p| p == cur) => {
            RoundCheck::RebasedSameChange
        }
        _ => RoundCheck::NoMatch,
    }
}

/// #3459 — pure: extract the `patch_id` values recorded on this card's
/// `demo.presented` witness lines. `card_key` is the `"card_id":<n>,` fragment the
/// witness uses. A line with no `patch_id` (a pre-#3459 demo) contributes nothing —
/// so an old demo can't match by patch-id, only by exact sha (correct: we can't
/// prove its change without the recorded id).
pub fn demoed_patch_ids(witness: &str, card_key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in witness.lines() {
        if !(l.contains("\"event\":\"demo.presented\"") && l.contains(card_key)) {
            continue;
        }
        if let Some(p) = json_str_field_raw(l, "patch_id") {
            if !p.is_empty() && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Minimal `"key":"value"` extractor over a raw JSONL line (std-only, no serde).
fn json_str_field_raw(line: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":\"", key);
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// #3459 — the card's diff identity: `git patch-id --stable` of `merge-base..HEAD`.
/// Stable across a clean rebase (same change ⇒ same id), unlike the commit-sha or
/// the tree (both move when the base moves). `None` if it can't be computed (empty
/// diff / git failure) — the caller treats None as no-match and refuses, never
/// silently allows. The `diff | patch-id` pipe runs via `bash -c` (subprocess only,
/// ADR-032 §1 — no code dep).
fn patch_id(werk_s: &str) -> Option<String> {
    let base = run_in(werk_s, "git", &["merge-base", "origin/main", "HEAD"]).ok()?;
    let base = base.trim();
    if base.is_empty() {
        return None;
    }
    let cmd = format!("git -C '{}' diff {}..HEAD | git patch-id --stable", werk_s, base);
    let out = run_in(werk_s, "bash", &["-c", &cmd]).ok()?;
    out.split_whitespace().next().filter(|s| !s.is_empty()).map(|s| s.to_string())
}

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
        jsonl(home, role, card, &trace, "merge.refused", ",\"reason\":\"no-werk\"");
        return Err(format!("no werk for #{} at {} — pull/commit/push the card first", card, werk.display()));
    }
    // werk must be on the card's branch (carries the #2580 cross-role intent).
    let cur = run_in(&werk_s, "git", &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    if cur != branch {
        jsonl(home, role, card, &trace, "merge.refused", ",\"reason\":\"branch-mismatch\"");
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
                jsonl(home, role, card, &trace, "merge.refused", ",\"reason\":\"pr-create-fail\"");
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
                jsonl(home, role, card, &trace, "merge.refused", ",\"reason\":\"no-open-pr\"");
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

    // #3365 — Jeff's step 5 gated by step 3: NO GO BEFORE ANNOUNCE, per round.
    // The round is the sha being landed (short=12, matching werk-demo's
    // current_round). A merge may only proceed when the witness carries a
    // demo.presented for THIS card and THIS round — i.e. the five-step
    // sequence reached the announce on these exact commits. A Jeff override
    // exists but is explicit and witnessed (CHORUS_GO_OVERRIDE=<reason>),
    // never ambient.
    {
        let round_key = format!("\"round\":\"{}\"", &head_sha[..12.min(head_sha.len())]);
        let card_key = format!("\"card_id\":{},", card);
        let demo_witness = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
        let announced = demo_witness.lines().any(|l| {
            l.contains("\"event\":\"demo.presented\"") && l.contains(&card_key) && l.contains(&round_key)
        });
        // #3459 — exact-sha round missed? Before refusing, check the card's DIFF.
        // A content-preserving rebase (a peer landed → the base moved) changes the
        // sha — and the tree — but NOT the change that was demoed, so the demoed
        // patch-id still matches. The sha churns on every rebase (the bug this
        // fixes); the patch-id does not. None ⇒ uncomputable ⇒ treated as no-match.
        let current_patch = if announced { None } else { patch_id(&werk_s) };
        let demoed_patches = if announced { Vec::new() } else { demoed_patch_ids(&demo_witness, &card_key) };
        match round_check(announced, current_patch.as_deref(), &demoed_patches) {
            RoundCheck::ExactSha => {}
            RoundCheck::RebasedSameChange => {
                let pid = current_patch.as_deref().unwrap_or("");
                jsonl(home, role, card, &trace, "merge.round-rebased", &format!(",\"patch_id\":\"{}\"", pid));
                emit_spine(home, "merge.round_rebased", role, card, &trace, &[("patch_id", pid)]);
            }
            RoundCheck::NoMatch => match env::var("CHORUS_GO_OVERRIDE") {
                Ok(reason) if !reason.trim().is_empty() => {
                    jsonl(home, role, card, &trace, "merge.override",
                          &format!(",\"reason\":\"announce-missing-this-round\",\"justification\":\"{}\"",
                                   reason.replace('"', "'")));
                    emit_spine(home, "merge.override", role, card, &trace,
                               &[("reason", "announce-missing-this-round")]);
                }
                _ => {
                    jsonl(home, role, card, &trace, "merge.refused", ",\"reason\":\"announce-missing-this-round\"");
                    return Err(format!(
                        "announce-missing-this-round: no demo.presented for #{} matches HEAD's round OR its diff — \
                         the change about to merge isn't the one demoed. If a rebase moved the sha but the change is \
                         identical, the patch-id should have matched (check werk-demo emits patch_id on demo.presented). \
                         If the content genuinely changed, re-prove it: run Half A, then go. \
                         (#3365/#3459; explicit override: CHORUS_GO_OVERRIDE=<reason>)",
                        card
                    ));
                }
            },
        }
    }

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
            jsonl(home, role, card, &trace, "merge.refused", &format!(",\"reason\":\"{}\"", reason));
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
        jsonl(home, role, card, &trace, "merge.refused", ",\"reason\":\"merge-fail\"");
        return Err(format!("merge-fail: pr #{} state is '{}', not MERGED — content did not land", pr, state));
    }
    if merge_oid.is_empty() || !oid_on_main(&werk_s, &merge_oid) {
        jsonl(home, role, card, &trace, "merge.refused", ",\"reason\":\"merge-fail\"");
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
