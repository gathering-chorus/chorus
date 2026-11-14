// @test-type: unit — pure: which changed test files carry no @domain header; no door.
// @domain: tests
// @card: #4965
//! #4965 AC6 (from #4935): werk-test refuses a changed test file with no `@domain:`
//! tag, naming the file. Before, a file with no header was not judged, and the
//! crawler reported test-declared-domain-missing only after it landed.
use werk_test::untagged_test_files;

fn file(p: &str, c: &str) -> (String, String) { (p.to_string(), c.to_string()) }

/// NEGATIVE PROOF (#3734) — the fixture with no tag is named
#[test]
fn a_test_with_no_domain_tag_is_named() {
    let changed = vec![file("platform/tests/no-tag.bats", "#!/usr/bin/env bats\n# @test-type: unit\n")];
    assert_eq!(untagged_test_files(&changed), vec!["platform/tests/no-tag.bats".to_string()]);
}

/// the same fixture with a valid tag passes
#[test]
fn the_same_file_with_a_tag_passes() {
    let changed = vec![file("platform/tests/no-tag.bats", "#!/usr/bin/env bats\n# @test-type: unit\n# @domain: tests\n")];
    assert!(untagged_test_files(&changed).is_empty());
}

#[test]
fn an_empty_tag_is_no_tag_and_non_tests_and_fixtures_are_not_judged() {
    let changed = vec![
        file("platform/pulse/src/a.test.ts", "// @domain:\n"),
        file("platform/scripts/tool", "#!/bin/bash\n"),
        file("platform/tests/fixtures/f.bats", "#!/usr/bin/env bats\n"),
    ];
    assert_eq!(untagged_test_files(&changed), vec!["platform/pulse/src/a.test.ts".to_string()]);
}
