//! #4229 — the SHACL report leg. Report-only by ruling (#3536 AC2), but its
//! three states have to stay apart: a crashed validator reporting "0" is
//! migration-complete-by-crash, and that is what this leg actually did on
//! every full deploy until #3731.
use athena_deploy::{shacl_violations, strip_bom, ShaclReport};

#[test]
fn violations_are_counted_from_the_report() {
    let out = "sh:resultSeverity sh:Violation ;\nsomething else\nsh:resultSeverity sh:Violation ;\n";
    assert_eq!(shacl_violations(out), 2);
    assert_eq!(shacl_violations("conforms: true\n"), 0);
}

#[test]
fn negative_proof_a_crash_is_not_zero_violations() {
    // The whole reason #3731 exists. If these two ever print the same, the
    // leg reports a clean model because the validator died.
    assert_eq!(ShaclReport::Ran { violations: 0 }.violations_field(), "0");
    assert_eq!(ShaclReport::Crashed.violations_field(), "unknown");
    assert_ne!(
        ShaclReport::Ran { violations: 0 }.violations_field(),
        ShaclReport::Crashed.violations_field()
    );
}

#[test]
fn negative_proof_an_absent_validator_is_not_a_clean_run_either() {
    assert_eq!(ShaclReport::ValidatorAbsent.violations_field(), "unknown");
    assert_ne!(ShaclReport::ValidatorAbsent.status_field(), ShaclReport::Ran { violations: 0 }.status_field());
}

#[test]
fn a_bom_is_stripped_from_every_member_not_just_the_one_that_had_it() {
    // A BOM is tolerated at the start of a file and fatal mid-union, which is
    // why this is per member rather than a fix to one .ttl (#4085).
    assert_eq!(strip_bom("\u{feff}@prefix c: <x> ."), "@prefix c: <x> .");
    assert_eq!(strip_bom("@prefix c: <x> ."), "@prefix c: <x> .");
    // Only at the start — a BOM later in the text is content, not a marker.
    assert_eq!(strip_bom("@prefix\u{feff} c: <x> ."), "@prefix\u{feff} c: <x> .");
}
