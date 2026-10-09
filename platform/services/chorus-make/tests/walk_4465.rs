//! #4465 AC7 — one query walks the chain (Jeff 2026-10-09 11:24): value stream →
//! its steps → the pipeline that automates it → the pipeline step covering each
//! stream step → that step's ordered skills → each skill's domain.
use chorus_make::{walk_check, walk_from_sparql_tsv, walk_query};
use std::process::Command;

fn dir() -> String {
    std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")
}

/// The walk's TSV over a fixture graph. No `sparql` on the box fails loudly.
fn run(stream: &str, data: &str) -> String {
    let q = format!("{}/walk-q-{}-{}.rq", std::env::temp_dir().display(), stream, std::process::id());
    std::fs::write(&q, walk_query(stream)).unwrap();
    let out = Command::new("sparql")
        .args(["--data", data, "--results", "TSV", "--query", &q])
        .output()
        .expect("Jena `sparql` must be on PATH for this test (brew install jena) — not skipped");
    let _ = std::fs::remove_file(&q);
    assert!(out.status.success(), "sparql failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

fn fixture() -> String {
    format!("{}/tests/fixtures/cicd-graph.trig", dir())
}

#[test]
fn the_werk_stream_walks_to_every_skill_and_its_domain() {
    let w = walk_from_sparql_tsv(&run("Werk", &fixture())).expect("walk");
    assert_eq!(w.pipeline, "cicd");
    // 9 stream steps; pull is covered by no pipeline step (it is /pull, before the pipeline)
    assert_eq!(w.uncovered, vec!["pull".to_string()]);
    let land: Vec<_> = w.rows.iter().filter(|r| r.stream_step == "merge").map(|r| r.skill.as_str()).collect();
    assert_eq!(land, ["werk-merge", "werk-sync", "werk-deploy --target canonical", "werk-accept"]);
    assert!(w.rows.iter().filter(|r| !r.skill.is_empty()).all(|r| !r.domain.is_empty()), "every skill names its domain");
    assert_eq!(walk_check(&w), Ok(()));
}

// NEGATIVE PROOF: drop one coversStep and one hasDomain — the check must go red
// and name both, not walk past them.
#[test]
fn a_missing_link_is_named_not_walked_past() {
    let broken = std::fs::read_to_string(fixture()).unwrap()
        .replace("chorus:pipeline-step-cicd-build chorus:coversStep chorus:value-stream-step-build .", "")
        .replace("chorus:skill-werk-test chorus:hasDomain chorus:tests .", "");
    let path = format!("{}/walk-broken-{}.trig", std::env::temp_dir().display(), std::process::id());
    std::fs::write(&path, broken).unwrap();
    let w = walk_from_sparql_tsv(&run("Werk", &path)).expect("walk");
    let _ = std::fs::remove_file(&path);
    let errs = walk_check(&w).expect_err("a broken chain must be red");
    let all = errs.join(" | ");
    assert!(all.contains("build"), "uncovered build step named: {all}");
    assert!(all.contains("werk-test"), "domainless skill named: {all}");
}

// A stream no pipeline automates is zero rows: refused, never an empty walk.
#[test]
fn an_unautomated_stream_is_refused() {
    let errs = walk_from_sparql_tsv(&run("Athena", &fixture())).expect_err("no pipeline automates it here");
    assert!(errs[0].contains("no pipeline automates"), "{errs:?}");
}
