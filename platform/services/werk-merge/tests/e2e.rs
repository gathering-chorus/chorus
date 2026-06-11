//! Real end-to-end for the atomic MERGE verb (#3175): actual `git` on temp repos +
//! a STATEFUL PATH-shimmed `gh` that records PRs and performs real squash-merges into
//! the temp origin. Proves the #3175 fix: resolve the PR by HEAD oid (not branch
//! name), land the CURRENT work, and content-verify the merge actually happened.
//!
//! One env-mutating test fn (PATH / ORIGIN / GH_STATE / GH_FAKE_MERGE), sequential
//! scenarios on fresh repos — same shape as werk-push's e2e.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_merge::merge;

// Both e2e fns mutate process env (PATH / ORIGIN / GH_*); serialize them.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}
fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wmerge-{}-{}-{}", tag, std::process::id(), nanos()));
    fs::create_dir_all(&p).unwrap();
    p
}
fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").args(args).current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .status().unwrap().success();
    assert!(ok, "git {:?} failed in {}", args, dir.display());
}
fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

/// origin (non-bare on main, accepts pushes) + home clone + a card worktree on the
/// card branch. Returns (origin, home, werk_base, werk).
fn scenario(role: &str, card: u64) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);
    let home = tmp("home");
    assert!(Command::new("git")
        .args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());
    let werk_base = tmp("werk");
    let werk = werk_base.join(format!("{}-{}", role, card));
    git(&home, &["worktree", "add", "-b", &format!("{}/{}", role, card), werk.to_str().unwrap(), "origin/main"]);
    (origin, home, werk_base, werk)
}

/// Commit a file in the werk and push the branch to origin (real git push).
fn commit_and_push(werk: &Path, branch: &str, file: &str, content: &str) -> String {
    fs::write(werk.join(file), content).unwrap();
    git(werk, &["add", "."]);
    git(werk, &["commit", "-q", "-m", &format!("add {}", file)]);
    git(werk, &["push", "-q", "origin", branch]);
    git_out(werk, &["rev-parse", "HEAD"])
}

fn origin_main_has(origin: &Path, file: &str) -> bool {
    Command::new("git")
        .args(["-C", origin.to_str().unwrap(), "cat-file", "-e", &format!("main:{}", file)])
        .status().unwrap().success()
}

#[test]
fn merge_resolves_by_oid_lands_real_work_and_content_verifies() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // #3365 — these scenarios test merge mechanics; the announce gate has its own
    // scenario below. Use the explicit witnessed override everywhere else.
    std::env::set_var("CHORUS_GO_OVERRIDE", "e2e-merge-mechanics");
    // ── stateful gh shim on PATH ───────────────────────────────────────────────
    let bin = tmp("bin");
    write_exec(&bin.join("gh"), GH_SHIM);
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));

    // ── Scenario A: no PR exists → create, squash-merge, content-verify ─────────
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9101);
        let sha = commit_and_push(&werk, "kade/9101", "a.txt", "alpha");
        let state = tmp("ghstate");
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", state.to_str().unwrap());
        std::env::remove_var("GH_FAKE_MERGE");

        let main_sha = merge(9101, "kade", &home, &werk_base).expect("merge creates PR, squashes, verifies");
        assert!(main_sha.len() >= 7, "returns the merged main sha");
        assert!(origin_main_has(&origin, "a.txt"), "scenario A: the work landed on origin/main");
        assert_ne!(sha, main_sha, "main moved to a NEW squash commit (not the branch tip)");
    }

    // ── Scenario B: Wren's bug — a STALE already-merged PR matches the branch NAME
    //    at an OLD sha. Resolving by name would false-green (reuse it, land nothing).
    //    Resolving by the current HEAD oid must create a FRESH PR and land the NEW
    //    work. ──────────────────────────────────────────────────────────────────
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9102);
        let _old = commit_and_push(&werk, "kade/9102", "first.txt", "v1");
        // Simulate the first PR already merged (state record at the OLD sha) WITHOUT
        // having advanced main for it — the point is merge() must not reuse it.
        let state = tmp("ghstate");
        fs::write(
            state.join("pr-440"),
            format!("PR_NUM=440\nPR_HEAD={}\nPR_BRANCH=kade/9102\nPR_STATE=merged\nPR_MERGE=deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\n", _old),
        ).unwrap();
        // New work on the SAME branch → new HEAD oid.
        let new_sha = commit_and_push(&werk, "kade/9102", "second.txt", "v2");
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", state.to_str().unwrap());
        std::env::remove_var("GH_FAKE_MERGE");

        let main_sha = merge(9102, "kade", &home, &werk_base)
            .expect("merge must create a fresh PR for the new HEAD oid, not reuse the stale #440");
        assert!(origin_main_has(&origin, "second.txt"), "scenario B: the NEW work landed (no false-green)");
        assert_ne!(new_sha, main_sha, "main advanced via squash, not just pointed at the branch");

        // ── Scenario B.2 idempotent: re-run on the same now-merged HEAD → no-op ──
        let again = merge(9102, "kade", &home, &werk_base).expect("idempotent re-merge is a no-op success");
        assert_eq!(again, main_sha, "idempotent re-run returns the same main sha, merges nothing new");
    }

    // ── Scenario D (#3266): GitHub's list API lags its create API — the #3269
    //    false-red. With a 2-call lag the bounded resolve-retry must absorb the
    //    race and land; the witness records the retries. ───────────────────────
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9104);
        commit_and_push(&werk, "kade/9104", "d.txt", "delta");
        let state = tmp("ghstate");
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", state.to_str().unwrap());
        std::env::set_var("GH_LIST_LAG", "2");
        std::env::remove_var("GH_FAKE_MERGE");

        let main_sha = merge(9104, "kade", &home, &werk_base)
            .expect("resolve-retry must absorb the create->list lag instead of refusing no-open-pr");
        assert!(main_sha.len() >= 7);
        assert!(origin_main_has(&origin, "d.txt"), "scenario D: the work landed despite list lag");
        let witness = fs::read_to_string(home.join("ops/logs/werk-merge.jsonl")).unwrap_or_default();
        assert!(witness.contains("merge.pr.resolve.retry"), "the retry is witnessed, not silent");
        std::env::remove_var("GH_LIST_LAG");
    }

    // ── Scenario C: content-verify guard — gh LIES (reports MERGED, lands nothing).
    //    The post-merge content-verify must catch it and REFUSE. ─────────────────
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9103);
        commit_and_push(&werk, "kade/9103", "c.txt", "gamma");
        let state = tmp("ghstate");
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", state.to_str().unwrap());
        std::env::set_var("GH_FAKE_MERGE", "1"); // gh reports merged with a bogus commit, lands nothing

        let r = merge(9103, "kade", &home, &werk_base);
        assert!(r.is_err(), "content-verify must REFUSE when gh reports MERGED but nothing landed");
        assert!(r.unwrap_err().contains("merge-fail"), "typed merge-fail refusal");
        assert!(!origin_main_has(&origin, "c.txt"), "guard held: nothing actually landed on main");
        std::env::remove_var("GH_FAKE_MERGE");
    }

    // ── Scenario D: branch-mismatch — werk on the wrong branch → refuse. ─────────
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9104);
        commit_and_push(&werk, "kade/9104", "d.txt", "delta");
        git(&werk, &["checkout", "-q", "-b", "kade/wrong"]);
        let state = tmp("ghstate");
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", state.to_str().unwrap());
        assert!(merge(9104, "kade", &home, &werk_base).is_err(), "wrong branch => refuse");
    }

    // ── Scenario E: no-werk — never pulled → refuse, no merge. ──────────────────
    {
        let (origin, home, werk_base, _werk) = scenario("kade", 9105);
        let state = tmp("ghstate");
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", state.to_str().unwrap());
        assert!(merge(9999, "kade", &home, &werk_base).is_err(), "no werk => refuse");
    }

    // ── Scenario G (#3365): NO GO BEFORE ANNOUNCE, per round. A merge without a
    //    demo.presented for THIS card at THIS round refuses typed; with the
    //    announce seeded for the exact round (sha[..12]) it proceeds. ─────────
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9365);
        let sha = commit_and_push(&werk, "kade/9365", "g.txt", "golf");
        let state = tmp("ghstate");
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", state.to_str().unwrap());
        std::env::remove_var("GH_FAKE_MERGE");
        std::env::remove_var("CHORUS_GO_OVERRIDE"); // the gate is LIVE here

        let r = merge(9365, "kade", &home, &werk_base);
        assert!(r.is_err(), "go without announce must refuse");
        assert!(r.unwrap_err().contains("announce-missing-this-round"), "typed refusal");
        assert!(!origin_main_has(&origin, "g.txt"), "nothing landed before the announce");

        // Seed the announce for THIS round → merge proceeds.
        fs::create_dir_all(home.join("ops/logs")).unwrap();
        fs::write(home.join("ops/logs/werk-demo.jsonl"), format!(
            "{{\"ts\":1,\"event\":\"demo.presented\",\"role\":\"kade\",\"card_id\":9365,\"trace_id\":\"t\",\"ac\":\"1/1\",\"round\":\"{}\",\"variant\":\"x\"}}\n",
            &sha[..12]
        )).unwrap();
        let landed = merge(9365, "kade", &home, &werk_base).expect("announce present => go proceeds");
        assert!(landed.len() >= 7);
        assert!(origin_main_has(&origin, "g.txt"), "the work landed after the announce");

        // Silas's ACK ask (#3365): SAME-ROUND RESUME ≠ STALE. A re-run go on the
        // already-merged sha (his live #3364 case: two go re-runs, same sha) must
        // no-op-pass via idempotency — the gate guards NEW merges, never recovery.
        let again = merge(9365, "kade", &home, &werk_base).expect("same-round resume passes (idempotent), never refuses as stale");
        assert_eq!(again, landed, "resume returns the same landed sha");
        std::env::set_var("CHORUS_GO_OVERRIDE", "e2e-merge-mechanics"); // restore for later scenarios
    }

    // ── Scenario F (#3336): CONTENT-VERIFY idempotency — a dropped-land resume where PR
    //    resolution MISSES (squash orphaned the oid AND the branch's PR records are gone,
    //    as if GitHub deleted the branch on merge). by-oid + open both return None, so the
    //    old code would refuse no-open-pr and strand the card in WIP. The content of HEAD
    //    is already on origin/main, so the new content-verify must NO-OP success. ─────────
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9106);
        commit_and_push(&werk, "kade/9106", "f.txt", "foxtrot");
        let state = tmp("ghstate");
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", state.to_str().unwrap());
        std::env::remove_var("GH_FAKE_MERGE");

        // First land: real squash-merge → f.txt is on origin/main (new squash sha).
        let landed = merge(9106, "kade", &home, &werk_base).expect("first land squash-merges");
        assert!(origin_main_has(&origin, "f.txt"), "scenario F: work landed on origin/main");

        // Simulate the dropped-land + branch deletion: wipe ALL PR state so neither the
        // merged-by-oid nor the open-PR resolution can find anything for this branch.
        for entry in fs::read_dir(&state).unwrap() {
            let p = entry.unwrap().path();
            if p.file_name().and_then(|s| s.to_str()).map(|s| s.starts_with("pr-")).unwrap_or(false) {
                fs::remove_file(p).unwrap();
            }
        }

        // Re-run (the dropped-land resume): no PR resolvable, but content IS on main →
        // content-verify no-ops to the SAME main sha instead of refusing no-open-pr.
        let again = merge(9106, "kade", &home, &werk_base)
            .expect("content-verify resume: no PR resolvable, but work is on main → no-op success");
        assert_eq!(again, landed, "resume returns the same main sha, merges nothing new");
        let witness = fs::read_to_string(home.join("ops/logs/werk-merge.jsonl")).unwrap_or_default();
        assert!(witness.contains(r#""reason":"content-on-main""#),
            "the content-verify idempotency is witnessed, not a silent pass");
    }
}

/// Stateful `gh` shim. Emulates the narrow surface werk-merge uses, over a real temp
/// ORIGIN repo. State lives in $GH_STATE/pr-<n> files.
const GH_SHIM: &str = r#"#!/bin/sh
# env: GH_STATE (dir), ORIGIN (origin repo), GH_FAKE_MERGE (optional)
cmd="$1"; sub="$2"; shift 2 2>/dev/null
case "$cmd" in
pr)
  case "$sub" in
  list)
    state=""; head=""
    while [ $# -gt 0 ]; do
      case "$1" in
        --state) state="$2"; shift 2;;
        --head)  head="$2";  shift 2;;
        --json)  shift 2;;
        *) shift;;
      esac
    done
    # GH_LIST_LAG: emulate GitHub's list API lagging create — return [] for the
    # first N list calls made AFTER a create (the #3269/#3266 false-red race).
    if [ -n "$GH_LIST_LAG" ] && [ -f "$GH_STATE/created" ]; then
      lagged=$(cat "$GH_STATE/lagged" 2>/dev/null || echo 0)
      if [ "$lagged" -lt "$GH_LIST_LAG" ]; then
        echo $((lagged+1)) > "$GH_STATE/lagged"
        printf '[]'
        exit 0
      fi
    fi
    out="["; first=1
    for f in "$GH_STATE"/pr-*; do
      [ -e "$f" ] || continue
      PR_NUM=; PR_HEAD=; PR_BRANCH=; PR_STATE=; PR_MERGE=; . "$f"
      [ "$PR_BRANCH" = "$head" ] || continue
      [ "$PR_STATE" = "$state" ] || continue
      [ $first -eq 1 ] || out="$out,"
      out="$out{\"number\":$PR_NUM,\"headRefOid\":\"$PR_HEAD\"}"
      first=0
    done
    printf '%s]' "$out"
    ;;
  create)
    head=""
    while [ $# -gt 0 ]; do
      case "$1" in --head) head="$2"; shift 2;; *) shift;; esac
    done
    sha=$(git -C "$ORIGIN" rev-parse "refs/heads/$head")
    n=$(cat "$GH_STATE/counter" 2>/dev/null || echo 100); n=$((n+1)); echo "$n" > "$GH_STATE/counter"
    {
      echo "PR_NUM=$n"
      echo "PR_HEAD=$sha"
      echo "PR_BRANCH=$head"
      echo "PR_STATE=open"
      echo "PR_MERGE="
    } > "$GH_STATE/pr-$n"
    : > "$GH_STATE/created"
    echo "https://github.test/pr/$n"
    ;;
  merge)
    num="$1"; f="$GH_STATE/pr-$num"
    PR_NUM=; PR_HEAD=; PR_BRANCH=; PR_STATE=; PR_MERGE=; . "$f"
    if [ -n "$GH_FAKE_MERGE" ]; then
      PR_MERGE=$(printf 'fake%s' "$num" | git hash-object --stdin)
    else
      git -C "$ORIGIN" checkout -q main
      git -C "$ORIGIN" merge --squash "$PR_BRANCH" >/dev/null 2>&1
      git -C "$ORIGIN" -c user.name=gh -c user.email=gh@x commit -q -m "Squash merge #$num" >/dev/null 2>&1
      PR_MERGE=$(git -C "$ORIGIN" rev-parse HEAD)
    fi
    {
      echo "PR_NUM=$PR_NUM"
      echo "PR_HEAD=$PR_HEAD"
      echo "PR_BRANCH=$PR_BRANCH"
      echo "PR_STATE=merged"
      echo "PR_MERGE=$PR_MERGE"
    } > "$f"
    echo "merged"
    ;;
  view)
    num="$1"
    PR_NUM=; PR_HEAD=; PR_BRANCH=; PR_STATE=; PR_MERGE=; . "$GH_STATE/pr-$num"
    up=$(printf '%s' "$PR_STATE" | tr 'a-z' 'A-Z')
    printf '{"state":"%s","mergeCommit":{"oid":"%s"}}' "$up" "$PR_MERGE"
    ;;
  esac
  ;;
api) exit 0;;
esac
exit 0
"#;

// #3330 (#3324 matrix, werk-merge gaps) — the --atomic approval gate proven to
// refuse BEFORE any side effect, merge.approved spine emit captured with the
// accepter, and the two PR-creation refusals (pr-create-fail / no-open-pr).
#[test]
fn atomic_gate_orders_before_side_effects_and_pr_refusals_are_typed() {
    let _env = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

    // #3365 — mechanics test; the announce gate is covered by Scenario G.
    std::env::set_var("CHORUS_GO_OVERRIDE", "e2e-atomic-mechanics");    // knob-wrapping gh shim: logs every call, fails/no-ops `pr create` on demand,
    // otherwise delegates to the stateful GH_SHIM (real squash into ORIGIN).
    let bin = tmp("bin2");
    write_exec(&bin.join("gh-real"), GH_SHIM);
    write_exec(&bin.join("gh"),
        "#!/bin/sh\necho \"$@\" >> \"$GH_CALLS\"\nif [ \"$1 $2\" = \"pr create\" ]; then\n  [ -n \"$GH_PR_CREATE_EXIT\" ] && exit \"$GH_PR_CREATE_EXIT\"\n  [ -n \"$GH_CREATE_NOOP\" ] && { echo noop; exit 0; }\nfi\nexec \"$(dirname \"$0\")/gh-real\" \"$@\"\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::remove_var("GH_PR_CREATE_EXIT");
    std::env::remove_var("GH_CREATE_NOOP");
    std::env::remove_var("GH_FAKE_MERGE");

    // ── --atomic with NO accepter refuses BEFORE any side effect: zero gh calls,
    // zero jsonl events for the card (the gate is FIRST in merge_inner — pin it).
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9401);
        let _sha = commit_and_push(&werk, "kade/9401", "a.txt", "alpha");
        let calls = tmp("calls1").join("log");
        std::env::set_var("GH_CALLS", calls.to_str().unwrap());
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", tmp("ghstate1").to_str().unwrap());
        let err = werk_merge::merge_atomic(9401, "kade", &home, &werk_base, None)
            .expect_err("--atomic without an accepter must refuse");
        assert!(err.to_lowercase().contains("accepter"), "names the missing accepter: {err}");
        assert!(fs::read_to_string(&calls).unwrap_or_default().is_empty(),
            "refusal ordered BEFORE side effects — zero gh calls");
        let witness = fs::read_to_string(home.join("ops/logs/werk-merge.jsonl")).unwrap_or_default();
        assert!(!witness.contains("9401"), "no witness events before the gate: {witness}");
    }

    // ── --atomic WITH an accepter lands, and merge.approved reaches the SPINE
    // carrying {accepter, pr, atomic} (ADR-037 {who,what,when} — was unasserted).
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9402);
        let _sha = commit_and_push(&werk, "kade/9402", "b.txt", "beta");
        std::env::set_var("GH_CALLS", tmp("calls2").join("log").to_str().unwrap());
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", tmp("ghstate2").to_str().unwrap());
        let log = home.join("platform/scripts/chorus-log");
        fs::create_dir_all(log.parent().unwrap()).unwrap();
        let cap = home.join("spine-capture.txt");
        write_exec(&log, &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\n", cap.display()));

        let main_sha = werk_merge::merge_atomic(9402, "kade", &home, &werk_base, Some("jeff".into()))
            .expect("--atomic with accepter lands");
        assert!(main_sha.len() >= 7);
        assert!(origin_main_has(&origin, "b.txt"), "work really landed on origin main");
        let emitted = fs::read_to_string(&cap).unwrap_or_default();
        assert!(emitted.contains("merge.approved") && emitted.contains("accepter=jeff")
            && emitted.contains("atomic=true"),
            "merge.approved on the spine with {{who,what,when}}: {emitted}");
    }

    // ── pr-create-fail: gh pr create exits non-zero → typed refusal, nothing merged.
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9403);
        let _sha = commit_and_push(&werk, "kade/9403", "c.txt", "gamma");
        std::env::set_var("GH_CALLS", tmp("calls3").join("log").to_str().unwrap());
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", tmp("ghstate3").to_str().unwrap());
        std::env::set_var("GH_PR_CREATE_EXIT", "1");
        let err = merge(9403, "kade", &home, &werk_base).expect_err("pr create failure must refuse");
        std::env::remove_var("GH_PR_CREATE_EXIT");
        assert!(err.contains("pr-create-fail"), "typed pr-create-fail: {err}");
        assert!(!origin_main_has(&origin, "c.txt"), "nothing landed");
    }

    // ── no-open-pr: create 'succeeds' but no PR is resolvable by oid → typed refusal.
    {
        let (origin, home, werk_base, werk) = scenario("kade", 9404);
        let _sha = commit_and_push(&werk, "kade/9404", "d.txt", "delta");
        std::env::set_var("GH_CALLS", tmp("calls4").join("log").to_str().unwrap());
        std::env::set_var("ORIGIN", origin.to_str().unwrap());
        std::env::set_var("GH_STATE", tmp("ghstate4").to_str().unwrap());
        std::env::set_var("GH_CREATE_NOOP", "1");
        let err = merge(9404, "kade", &home, &werk_base).expect_err("unresolvable PR must refuse");
        std::env::remove_var("GH_CREATE_NOOP");
        assert!(err.contains("no-open-pr"), "typed no-open-pr: {err}");
        assert!(!origin_main_has(&origin, "d.txt"), "nothing landed");
    }
}
