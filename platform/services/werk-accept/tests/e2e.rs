//! Real end-to-end for the accept verb (#3057): actual `git` on temp repos +
//! PATH-shimmed `gh` / `cards` / `chorus-log` / `chorus-werk`. Proves (1) the
//! authority gate refuses end-to-end with NO finalize side effects, and (2) the
//! happy path finalizes (merge + cards-done called) when DEPLOY_ROLE=jeff.
//! One env-mutating test fn so PATH / shim-config can't race.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_accept::accept;

fn nanos() -> u128 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() }
fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wa-{}-{}-{}", tag, std::process::id(), nanos()));
    fs::create_dir_all(&p).unwrap();
    p
}
fn git(dir: &Path, args: &[&str]) {
    assert!(Command::new("git").args(args).current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .status().unwrap().success(), "git {:?} failed", args);
}
fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

#[test]
fn accept_authority_gate_and_happy_path() {
    // #3183: werk-accept now resolves cards/chorus-log/chorus-werk ABSOLUTELY under
    // home/platform/scripts, and werk-deploy under $CHORUS_BIN — independent of PATH
    // (it's exec'd by the chorus-mcp daemon whose PATH lacks platform/scripts; bare
    // names died "No such file or directory" accepting #3211). So the chorus shims go
    // at their ABSOLUTE locations (below, once home exists) and stay OFF PATH; only gh
    // (a real system tool, called bare) sits on PATH. If accept used bare names it
    // would fail to find `cards` here — passing proves the absolute resolution.
    let bin = tmp("bin");
    let log = tmp("log").join("calls");
    std::env::set_var("SHIM_LOG", log.to_str().unwrap());
    write_exec(&bin.join("gh"), "#!/bin/sh\necho \"gh $@\" >> \"$SHIM_LOG\"\nexit \"${GH_EXIT:-0}\"\n");
    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    let chorus_bin = tmp("chorus-bin");
    std::env::set_var("CHORUS_BIN", chorus_bin.to_str().unwrap());

    // real git: origin + home clone + a card worktree with a PUSHED commit.
    let origin = tmp("origin");
    git(&origin, &["init", "-q", "-b", "main", "."]);
    fs::write(origin.join("README"), "x").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    git(&origin, &["config", "receive.denyCurrentBranch", "ignore"]);
    let home = tmp("home");
    assert!(Command::new("git").args(["clone", "-q", origin.to_str().unwrap(), home.to_str().unwrap()])
        .status().unwrap().success());
    let werk_base = tmp("werk");
    let werk = werk_base.join("kade-9001");
    git(&home, &["worktree", "add", "-b", "kade/9001", werk.to_str().unwrap(), "origin/main"]);
    fs::write(werk.join("w.txt"), "x").unwrap();
    git(&werk, &["add", "."]);
    git(&werk, &["commit", "-q", "-m", "work"]);
    git(&werk, &["push", "-q", "origin", "kade/9001"]);

    // #3183: place the chorus shims at the ABSOLUTE paths accept resolves — home/
    // platform/scripts (cards/chorus-log/chorus-werk) + $CHORUS_BIN (werk-deploy) —
    // and NOT on PATH. accept finds them by absolute resolution, not the daemon PATH.
    let scripts = home.join("platform/scripts");
    fs::create_dir_all(&scripts).unwrap();
    write_exec(&scripts.join("cards"),
        "#!/bin/sh\ncase \"$1\" in\n view) echo \"{ \\\"status\\\": \\\"${CARDS_STATUS:-WIP}\\\", \\\"owner\\\": \\\"${CARDS_OWNER:-kade}\\\" }\" ;;\n done) echo \"cards done $2\" >> \"$SHIM_LOG\" ;;\nesac\n");
    write_exec(&scripts.join("chorus-log"), "#!/bin/sh\necho \"chorus-log $@\" >> \"$SHIM_LOG\"\n");
    write_exec(&scripts.join("chorus-werk"), "#!/bin/sh\necho \"chorus-werk $@\" >> \"$SHIM_LOG\"\n");
    write_exec(&chorus_bin.join("werk-deploy"), "#!/bin/sh\necho \"werk-deploy $@\" >> \"$SHIM_LOG\"\n");

    std::env::set_var("CARDS_OWNER", "kade");
    std::env::set_var("CARDS_STATUS", "WIP");

    // (1) authority gate: a builder (kade) self-accepting => Err, NO finalize.
    assert!(accept(9001, "kade", "kade", &home, &werk_base).is_err(), "builder self-accept refused");
    let after_refuse = fs::read_to_string(&log).unwrap_or_default();
    assert!(!after_refuse.contains("pr merge"), "refused accept must NOT merge");
    assert!(!after_refuse.contains("cards done"), "refused accept must NOT mark done");
    // #3108 AC#8: env-down sits post-merge; a refused accept must not reach it.
    assert!(!after_refuse.contains("werk-deploy env-down"), "refused accept must NOT tear down variants");

    // (2) happy path: DEPLOY_ROLE=jeff accepting kade's card => Ok, FINALIZE only.
    accept(9001, "kade", "jeff", &home, &werk_base).expect("jeff finalizes");
    let after_ok = fs::read_to_string(&log).unwrap_or_default();
    // #3175: accept is FINALIZE-ONLY — werk-merge owns the merge. accept must NOT merge.
    assert!(!after_ok.contains("pr merge"), "accept no longer merges — werk-merge owns the merge (#3175)");
    assert!(!after_ok.contains("pr create"), "accept no longer opens a PR — that's werk-merge's job");
    assert!(after_ok.contains("cards done 9001"), "happy path marks the card Done");
    assert!(after_ok.contains("chorus-log card.accepted"), "happy path emits card.accepted");
    // #3108 AC#8: env-down called on happy path AND precedes chorus-werk remove
    // (so variant services release file handles before the worktree is destroyed).
    assert!(after_ok.contains("werk-deploy env-down kade"), "happy path tears down kade's variant");
    let env_down_at = after_ok.find("werk-deploy env-down").expect("env-down logged");
    let werk_remove_at = after_ok.find("chorus-werk remove kade 9001").expect("chorus-werk remove logged");
    assert!(env_down_at < werk_remove_at, "env-down must precede chorus-werk remove (got env-down at {}, werk remove at {})", env_down_at, werk_remove_at);

    // (3) the load-bearing nuance: wren accepting a WREN-owned card => Err (self-accept).
    std::env::set_var("CARDS_OWNER", "wren");
    let werk2 = werk_base.join("wren-9002");
    git(&home, &["worktree", "add", "-b", "wren/9002", werk2.to_str().unwrap(), "origin/main"]);
    fs::write(werk2.join("w.txt"), "y").unwrap();
    git(&werk2, &["add", "."]);
    git(&werk2, &["commit", "-q", "-m", "w"]);
    git(&werk2, &["push", "-q", "origin", "wren/9002"]);
    assert!(accept(9002, "wren", "wren", &home, &werk_base).is_err(), "wren self-accept refused");
}
