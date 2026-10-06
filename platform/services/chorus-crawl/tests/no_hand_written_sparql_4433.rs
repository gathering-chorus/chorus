// @test-type: unit — reads this crate's own source; no services.
//! #4433 — the crawler reads and writes the graph only through the API.
//! Jeff, 2026-10-05: "we must not craft custom sparql to do what apis do".
//! The crawler had two hand-written queries straight to the store; this fails
//! if one comes back. (`3030` alone is not banned: the test classifier in
//! cases.rs looks for it in OTHER files' source to tell a test that touches
//! the store from a hermetic one.)

use std::fs;
use std::path::Path;

const BANNED: &[&str] = &[
    "SELECT ",
    "PREFIX chorus",
    "GRAPH <",
    "INSERT DATA",
    "DELETE WHERE",
    "FUSEKI_QUERY",
    "localhost:3030/pods",
];

fn offending_lines(file: &str, text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| BANNED.iter().any(|b| l.contains(b)))
        .map(|(i, l)| format!("{file}:{}: {}", i + 1, l.trim()))
        .collect()
}

#[test]
fn the_crawler_source_holds_no_sparql_and_no_store_url() {
    let src = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src");
    let mut files = 0;
    let mut hits = Vec::new();
    for entry in fs::read_dir(&src).expect("read src/") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        files += 1;
        let text = fs::read_to_string(&path).unwrap();
        hits.extend(offending_lines(&path.display().to_string(), &text));
    }
    // A renamed or emptied src/ must fail, not pass for having nothing to read.
    assert!(files >= 4, "expected the crawler's source files under {}, found {files}", src.display());
    assert!(hits.is_empty(), "hand-written SPARQL or a store URL in the crawler — ask the API:\n{}", hits.join("\n"));
}

// NEGATIVE PROOF — the two queries this card removed are caught.
#[test]
fn the_removed_queries_would_be_caught() {
    let old_results = r#"format!("PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?r WHERE {{ GRAPH <urn:chorus:domains:tests> {{ ?r chorus:ofTest <x> }} }}")"#;
    let old_domains = r#"let url = std::env::var("FUSEKI_QUERY").unwrap_or_else(|_| "http://localhost:3030/pods/query".to_string());"#;
    assert_eq!(offending_lines("a.rs", old_results).len(), 1);
    assert_eq!(offending_lines("b.rs", old_domains).len(), 1);
    assert!(offending_lines("c.rs", "near(c, \"post\", 40, \"3030\")").is_empty());
}
