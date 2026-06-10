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
use werk_accept::{finalize, signal};

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
fn signal_then_finalize_one_exit_verb() {
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

    // === #3237 split → #3311 consolidation: ONE exit verb. signal (authority-gated,
    // writes the decision) and finalize (mechanical close) are the two halves the
    // werk-accept BINARY now runs in sequence (GO = accept). werk-do-more and the
    // werk-finalize binary are DELETED — no-go/more = do nothing (#3279), and the
    // close has no standalone door. These tests pin each half's contract.
    let demo_witness = home.join("ops/logs/werk-demo.jsonl");

    // (1) signal AUTHORITY: a builder (kade) self-signaling => Err, and NO decision
    //     written — the seam stays blocked; werk-demo never sees a go it shouldn't.
    assert!(signal(9001, "kade", "kade", &home).is_err(), "builder self-accept refused on signal");
    assert!(
        !demo_witness.exists() || !fs::read_to_string(&demo_witness).unwrap().contains("demo.decision"),
        "refused signal must write NO demo.decision"
    );

    // (2) signal GO: DEPLOY_ROLE=jeff signaling kade's WIP card => Ok, writes the
    //     byte-exact demo.decision{go} (comma-terminated) and does NOT finalize.
    signal(9001, "kade", "jeff", &home).expect("jeff signals go");
    let w = fs::read_to_string(&demo_witness).unwrap_or_default();
    assert!(
        w.contains("\"event\":\"demo.decision\"") && w.contains("\"card_id\":9001,") && w.contains("\"decision\":\"go\""),
        "signal writes byte-exact demo.decision go; got:\n{}", w
    );
    let after_signal = fs::read_to_string(&log).unwrap_or_default();
    assert!(!after_signal.contains("cards done"), "signal must NOT finalize (no cards done)");
    assert!(!after_signal.contains("werk-deploy env-down"), "signal must NOT tear down variants");
    assert!(!after_signal.contains("pr merge"), "signal never merges");

    // (3) finalize VERDICT GATE: no demo.verdict=pass yet (the go wrote a decision, not a
    //     verdict — werk-demo writes the verdict on go) => Err, NO finalize.
    assert!(finalize(9001, "kade", &home).is_err(), "finalize refuses without demo.verdict=pass");
    assert!(!fs::read_to_string(&log).unwrap_or_default().contains("cards done 9001"),
        "verdict-gate refusal must NOT finalize");
    // record the verdict werk-demo would have written on go.
    fs::write(&demo_witness, format!(
        "{}{{\"ts\":1,\"event\":\"demo.verdict\",\"role\":\"kade\",\"card_id\":9001,\"trace_id\":\"t\",\"verdict\":\"pass\"}}\n",
        fs::read_to_string(&demo_witness).unwrap()
    )).unwrap();

    // (4) finalize HAPPY: mechanical close — runs inside accept after the gated signal.
    finalize(9001, "kade", &home).expect("finalize runs post-deploy");
    let after_fin = fs::read_to_string(&log).unwrap_or_default();
    assert!(after_fin.contains("cards done 9001"), "finalize marks the card Done");
    assert!(after_fin.contains("chorus-log card.accepted"), "finalize emits card.accepted");
    assert!(after_fin.contains("werk-deploy env-down kade"), "finalize tears down kade's variant");
    let env_down_at = after_fin.find("werk-deploy env-down").expect("env-down logged");
    let werk_remove_at = after_fin.find("chorus-werk remove kade 9001").expect("chorus-werk remove logged");
    assert!(env_down_at < werk_remove_at, "env-down precedes chorus-werk remove");
    assert!(!after_fin.contains("pr merge"), "finalize never merges (werk-merge owns merge)");

    // (5) signal self-accept nuance: wren signaling a WREN-owned card => Err. signal
    //     doesn't touch the werk, so no worktree setup is needed.
    std::env::set_var("CARDS_OWNER", "wren");
    assert!(signal(9002, "wren", "wren", &home).is_err(), "wren self-accept refused on signal");
}
