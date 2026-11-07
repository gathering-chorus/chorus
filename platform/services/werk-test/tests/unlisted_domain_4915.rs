// @test-type: unit — pure: header parsing and the listed-domain check; no door.
// @domain: tests
// @card: #4915
//! #4915 round 2 (Jeff's go 11-07 08:47): werk-test refuses a changed test whose
//! `@domain:` names a domain the store does not list. On 11-07, 19 such files had
//! landed (proving ×15, chorus ×3, autonomy ×1) and #4919 then landed 3 more (werk).
use werk_test::{declared_domain, unlisted_test_domains};

fn listed() -> Vec<String> { ["cicd", "roles", "tests"].iter().map(|s| s.to_string()).collect() }
fn file(p: &str, c: &str) -> (String, String) { (p.to_string(), c.to_string()) }

#[test]
fn negative_proof_an_unlisted_domain_is_named() {
    let changed = vec![
        file("platform/tests/x.bats", "#!/usr/bin/env bats\n# @domain: nowhere\n"),
        file("platform/services/a/tests/y.rs", "// @domain: proving\n#[test] fn t() {}\n"),
    ];
    let bad = unlisted_test_domains(&changed, &listed());
    assert_eq!(bad, vec![
        ("platform/tests/x.bats".to_string(), "nowhere".to_string()),
        ("platform/services/a/tests/y.rs".to_string(), "proving".to_string()),
    ]);
}

#[test]
fn a_listed_domain_passes() {
    let changed = vec![
        file("platform/tests/x.bats", "# @domain: cicd — the product domain this suite guards\n"),
        file("platform/pulse/src/a.test.ts", "// @Domain:  Roles\n"),
    ];
    assert!(unlisted_test_domains(&changed, &listed()).is_empty());
}

#[test]
fn only_test_files_are_judged() {
    // scripts carry @domain too (domain-radius says `ontology`); the crawler checks tests only
    let changed = vec![
        file("platform/scripts/domain-radius", "# @domain: ontology\n"),
        file("platform/tests/fixtures/f.bats", "# @domain: nowhere\n"),
        file("platform/tests/no-header.bats", "#!/usr/bin/env bats\n"),
    ];
    assert!(unlisted_test_domains(&changed, &listed()).is_empty());
}

#[test]
fn the_header_rule_matches_the_crawlers() {
    assert_eq!(declared_domain("#!/usr/bin/env bats\n# @test-type: unit\n# @domain: messages\n").as_deref(), Some("messages"));
    assert_eq!(declared_domain(" * @Domain:  Tests — why\n").as_deref(), Some("tests"));
    assert_eq!(declared_domain("{\n  \"@domain\": \"spine\",\n").as_deref(), Some("spine"));
    assert_eq!(declared_domain("# @domain:\n"), None);
    let late = format!("{}\n# @domain: cicd\n", "x".repeat(2100));
    assert_eq!(declared_domain(&late), None, "only the first 2,000 chars count");
}
