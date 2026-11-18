// @test-type: integration — the name-prefix retirement's own SPARQL on a scratch Fuseki loaded with fixture rows
// @domain: domains
// @card: #5002
//! #5002 AC4 (Jeff 11-18 10:23; guards Silas 10:28/10:33) — `retire_name_prefix` deletes the
//! instance rows whose door name starts with the prefix and the provenance Versions that name
//! such a row, never a real row with the prefix mid-name, never the ontology, security or
//! provenance rows themselves, and refuses above either ceiling or with inbound edges.

use athena_deploy::{ceiling_refusal, name_prefix_inbound_query, name_prefix_ok, name_prefix_retirement_sparql, name_prefix_window, parse_retirement, retirement_action, run_retirement_on, RetireAction, StoreCtx};
use std::process::Command;

const C: &str = "https://jeffbridwell.com/chorus#";

fn boot() -> Option<(std::process::Child, u16, std::path::PathBuf)> {
    let home = match std::env::var("FUSEKI_HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => std::path::PathBuf::from(std::env::var("HOME").ok()?).join(".gathering/data/fuseki-5.1.0"),
    };
    let jar = home.join("fuseki-server.jar");
    if !jar.exists() { return None; }
    let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    let dir = std::env::temp_dir().join(format!("athena-deploy-5002-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("config.ttl"), "@prefix fuseki: <http://jena.apache.org/fuseki#> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix ja: <http://jena.hpl.hp.com/2005/11/Assembler#> .\n[] rdf:type fuseki:Server ; fuseki:services ( <#s> ) .\n<#s> a fuseki:Service ; fuseki:name \"t\" ; fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name \"query\" ] ; fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name \"update\" ] ; fuseki:dataset [ a ja:MemoryDataset ] .\n").ok()?;
    let java = std::env::var("JAVA").unwrap_or_else(|_| "/opt/homebrew/opt/openjdk/bin/java".into());
    let child = Command::new(java).current_dir(&dir).env("FUSEKI_BASE", &dir).env("FUSEKI_HOME", &home)
        .args(["-Xmx256m", "-cp", jar.to_str()?, "org.apache.jena.fuseki.cmd.FusekiCmd", "--config=config.ttl", &format!("--port={port}")])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok()?;
    for _ in 0..180 {
        if Command::new("curl").args(["-sf", "-o", "/dev/null", "--max-time", "1", &format!("http://127.0.0.1:{port}/$/ping")]).status().is_ok_and(|s| s.success()) {
            return Some((child, port, dir));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    None
}

fn update(port: u16, q: &str) {
    let ok = Command::new("curl").args(["-sf", "--max-time", "30", "--data-urlencode", &format!("update={q}"), &format!("http://127.0.0.1:{port}/t/update")])
        .status().is_ok_and(|s| s.success());
    assert!(ok, "update failed: {q}");
}

fn count(port: u16, q: &str) -> usize {
    let out = Command::new("curl").args(["-sf", "--max-time", "30", "-H", "Accept: text/csv", "--data-urlencode", &format!("query={q}"), &format!("http://127.0.0.1:{port}/t/query")])
        .output().expect("curl");
    String::from_utf8_lossy(&out.stdout).lines().nth(1).and_then(|l| l.trim().parse().ok()).expect("a count")
}

fn ask(port: u16, s: &str) -> bool {
    count(port, &format!("SELECT (COUNT(*) AS ?n) WHERE {{ GRAPH ?g {{ <{C}{s}> ?p ?o }} }}")) > 0
}

const FIXTURE: &str = "PREFIX c: <https://jeffbridwell.com/chorus#>
INSERT DATA {
  GRAPH <urn:chorus:domains:code> {
    c:language-zz-probe-a a c:Language ; c:label \"probe a\" .
    c:code-kind-zz-probe-b a c:CodeKind ; c:label \"probe b\" .
    c:language-rust a c:Language ; c:label \"rust\" .
    c:language-mything-zz-probe-x a c:Language ; c:label \"a real row with the prefix mid-name\" .
  }
  GRAPH <urn:chorus:domains:security> { c:principal-zz-probe-c a c:Principal . }
  GRAPH <urn:chorus:ontology> { c:language-zz-probe-o a c:Language . }
  GRAPH <urn:chorus:domains:provenance> {
    c:version-a a c:Version ; c:ofRow \"languages/zz-probe-a\" .
    c:version-gone a c:Version ; c:ofRow \"languages/zz-probe-gone\" .
    c:version-rust a c:Version ; c:ofRow \"languages/rust\" .
    c:version-mid a c:Version ; c:ofRow \"languages/mything-zz-probe-x\" .
  }
}";

#[test]
fn the_prefix_takes_probe_rows_and_their_versions_and_nothing_else() {
    let (mut child, port, dir) = boot().expect("no scratch Fuseki: ~/.gathering/data/fuseki-5.1.0 and java are needed, or it did not answer in 90s");
    update(port, FIXTURE);
    let [rows, vers] = name_prefix_retirement_sparql("zz-probe-");
    let (n_rows, n_vers) = (count(port, &rows.0), count(port, &vers.0));
    assert_eq!(inbound_then_clear(port), (1, 0), "an edge from a real row into a probe row is counted, and refuses");
    update(port, &rows.3);
    update(port, &vers.3);
    let survivors = ["language-rust", "language-mything-zz-probe-x", "principal-zz-probe-c", "language-zz-probe-o", "version-rust", "version-mid"]
        .map(|s| (s, ask(port, s)));
    let gone = ["language-zz-probe-a", "code-kind-zz-probe-b", "version-a", "version-gone"].map(|s| (s, ask(port, s)));
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!((n_rows, n_vers), (2, 2), "2 probe rows (code graph), 2 versions (one orphan)");
    for (s, here) in survivors { assert!(here, "{s} must survive"); }
    for (s, here) in gone { assert!(!here, "{s} must be gone"); }
}

/// The inbound guard on the fixture: add one real edge into a probe row, count it, take it back.
fn inbound_then_clear(port: u16) -> (usize, usize) {
    let q = name_prefix_inbound_query("zz-probe-");
    update(port, &format!("INSERT DATA {{ GRAPH <urn:chorus:domains:code> {{ <{C}language-rust> <{C}dependsOn> <{C}language-zz-probe-a> }} }}"));
    let with_edge = count(port, &q);
    update(port, &format!("DELETE DATA {{ GRAPH <urn:chorus:domains:code> {{ <{C}language-rust> <{C}dependsOn> <{C}language-zz-probe-a> }} }}"));
    (with_edge, count(port, &q))
}

/// NEGATIVE PROOFS: over a ceiling, an unanswered count and a missing ceiling all refuse; at
/// the ceiling passes; an unsafe prefix is never accepted.
#[test]
fn ceilings_and_prefixes_refuse_fail_closed() {
    assert_eq!(ceiling_refusal("rows", Some(260), "260"), None);
    assert_eq!(ceiling_refusal("rows", Some(261), "260").as_deref(), Some("name-prefix-rows-over-ceiling:261>260"));
    assert!(ceiling_refusal("versions", None, "12100").is_some(), "an unanswered count refuses");
    assert!(ceiling_refusal("versions", Some(1), "").is_some(), "no ceiling refuses");
    assert!(name_prefix_ok("zz-probe-"));
    for bad in ["", "z", "zz-", "zz-probe", "ZZ-probe-", "zz probe-", "zz\"-probe-"] {
        assert!(!name_prefix_ok(bad), "{bad:?} must be refused");
    }
}

/// The ledger line becomes this action, with both ceilings carried.
#[test]
fn a_ledger_line_parses_to_the_name_prefix_action() {
    let line = r#"{"retire_name_prefix": "zz-probe-", "rows_ceiling": "260", "versions_ceiling": "12100", "until": "2026-11-25", "reason": "x", "by": "wren", "card": "5002", "status": "staged"}"#;
    let r = parse_retirement(line).unwrap().unwrap();
    assert_eq!(retirement_action(&r, "urn:x"), RetireAction::NamePrefix {
        prefix: "zz-probe-".into(), rows_ceiling: "260".into(), versions_ceiling: "12100".into(), until: "2026-11-25".into(),
    });
}

fn action(rows: &str, vers: &str) -> RetireAction {
    RetireAction::NamePrefix { prefix: "zz-probe-".into(), rows_ceiling: rows.into(), versions_ceiling: vers.into(), until: "2026-11-25".into() }
}

/// Silas 10:36 — the real path: a refused ceiling deletes nothing, the dry run deletes nothing,
/// and the run itself backs up both kinds before it deletes them.
#[test]
fn the_run_refuses_over_a_ceiling_dry_runs_and_backs_up_before_deleting() {
    let (mut child, port, dir) = boot().expect("no scratch Fuseki: ~/.gathering/data/fuseki-5.1.0 and java are needed, or it did not answer in 90s");
    update(port, FIXTURE);
    let backups = dir.join("backups");
    std::env::set_var("GRAPH_BACKUP_DIR", &backups);
    let today = "2026-11-18";
    let log = dir.join("spine.log");
    let logger = dir.join("chorus-log");
    std::fs::write(&logger, format!("#!/bin/sh\necho \"$*\" >> {}\n", log.display())).unwrap();
    std::process::Command::new("chmod").args(["+x", logger.to_str().unwrap()]).status().unwrap();
    let ctx = StoreCtx {
        gsp: format!("http://127.0.0.1:{port}/t/data"),
        query: format!("http://127.0.0.1:{port}/t/query"),
        update: format!("http://127.0.0.1:{port}/t/update"),
        chorus_log: logger.to_string_lossy().into(),
        role: "test".into(),
    };
    // #5002 (quality review) — the inbound-edge refusal through the real run: a real row that
    // points at a probe stops the run, and nothing is deleted
    update(port, &format!("INSERT DATA {{ GRAPH <urn:chorus:domains:code> {{ <{C}language-rust> <{C}dependsOn> <{C}language-zz-probe-a> }} }}"));
    let inbound = run_retirement_on(&action("260", "12100"), 0, &ctx, today);
    let after_inbound = ask(port, "language-zz-probe-a");
    update(port, &format!("DELETE DATA {{ GRAPH <urn:chorus:domains:code> {{ <{C}language-rust> <{C}dependsOn> <{C}language-zz-probe-a> }} }}"));
    // past its until, the line is spent: nothing is deleted
    let spent = run_retirement_on(&action("260", "12100"), 0, &ctx, "2026-11-26");
    let spine = std::fs::read_to_string(&log).unwrap_or_default();
    let after_spent = ask(port, "language-zz-probe-a");
    let over_rows = run_retirement_on(&action("1", "12100"), 1, &ctx, today);
    let over_vers = run_retirement_on(&action("260", "1"), 2, &ctx, today);
    let after_refusals = (ask(port, "language-zz-probe-a"), ask(port, "version-gone"));
    std::env::set_var("ATHENA_RETIRE_DRY_RUN", "1");
    let dry = run_retirement_on(&action("260", "12100"), 3, &ctx, today);
    std::env::remove_var("ATHENA_RETIRE_DRY_RUN");
    let after_dry = (ask(port, "language-zz-probe-a"), ask(port, "version-gone"));
    let real = run_retirement_on(&action("260", "12100"), 4, &ctx, today);
    let after_run = (ask(port, "language-zz-probe-a"), ask(port, "version-gone"), ask(port, "language-mything-zz-probe-x"));
    let dumped: Vec<usize> = std::fs::read_dir(&backups).map(|d| d.flatten()
        .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default().lines().filter(|l| !l.trim().is_empty()).count()).collect()).unwrap_or_default();
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(inbound.as_ref().is_err_and(|e| e.contains("name-prefix-rows-have-inbound-edges")), "{inbound:?}");
    assert!(after_inbound, "an inbound-edge refusal deleted the probe");
    assert!(spent.is_ok() && after_spent, "a spent line must do nothing: {spent:?}");
    // a spent line says so on the spine (code review 11-18); NEGATIVE PROOF: the in-window runs
    // above never emit it, so the line is spent-only
    assert_eq!(spine.matches("model.retirement.skipped").count(), 1, "spent must emit exactly one skipped event: {spine}");
    assert!(spine.contains("spent: past until 2026-11-25"), "{spine}");
    // NEGATIVE PROOFS: each ceiling refuses on its own, and a refusal deletes nothing
    assert!(over_rows.as_ref().is_err_and(|e| e.contains("name-prefix-rows-over-ceiling:2>1")), "{over_rows:?}");
    assert!(over_vers.as_ref().is_err_and(|e| e.contains("name-prefix-versions-over-ceiling:2>1")), "{over_vers:?}");
    assert_eq!(after_refusals, (true, true), "a refused ceiling deleted something");
    assert!(dry.is_ok(), "{dry:?}");
    assert_eq!(after_dry, (true, true), "the dry run deleted something");
    assert!(real.is_ok(), "{real:?}");
    assert_eq!(after_run, (false, false, true), "probe row and orphan Version gone, the mid-name real row kept");
    assert_eq!(dumped.len(), 2, "one backup per kind: {dumped:?}");
    assert!(dumped.iter().all(|n| *n > 0), "each backup holds the deleted triples: {dumped:?}");
}

/// #5002 — the window: inside runs, past is spent, missing or malformed refuses.
#[test]
fn the_until_window_runs_spends_and_refuses() {
    assert_eq!(name_prefix_window("2026-11-25", "2026-11-25"), Ok(true));
    assert_eq!(name_prefix_window("2026-11-25", "2026-11-26"), Ok(false));
    assert!(name_prefix_window("", "2026-11-18").is_err(), "no until refuses");
    assert!(name_prefix_window("2026-11-5", "2026-11-18").is_err(), "a malformed until refuses");
    assert!(name_prefix_window("2026-11-25", "").is_err(), "an unreadable today refuses, never runs");
}
