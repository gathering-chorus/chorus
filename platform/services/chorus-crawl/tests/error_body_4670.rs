// @test-type: unit — hermetic: the error text a failed door call reports, and what the prefix learner reads from it; no store.
// @domain: code
// @card: #4670
// @owner: kade
//! #4670 — a double-prefix refusal for a long link value must still teach the
//! crawler the prefix, or the write is never retried and the pass stays red.
use chorus_crawl::{error_body, PrefixMemory};

fn refusal() -> String {
    // the 10-21 09:08 body, shape and length as the door sent it
    let v = "code-file-file-building-products-convergence-domains-integrations-icd-instances-icd-instance-documents-ttl-eff0629a";
    format!(r#"{{ "error": "validation", "message": "athena-model: double-prefix: '{v}' already starts with 'code-file-' — pass the bare name; the mint adds the prefix (ADR-040 Rule 0)" }}"#)
}

#[test]
fn a_long_refusal_still_teaches_the_prefix() {
    assert!(refusal().len() > 200);
    let mut m = PrefixMemory::default();
    assert!(m.learn(&error_body(&refusal())), "{}", error_body(&refusal()));
    let mut f = vec![("exercises".to_string(), "code-file-file-x-1".to_string())];
    assert!(m.apply(&mut f));
    assert_eq!(f[0].1, "file-x-1");
}

// NEGATIVE PROOF: the old 200-char cut, on the same body, taught nothing.
#[test]
fn the_200_char_cut_alone_teaches_nothing() {
    let cut: String = refusal().chars().take(200).collect();
    assert!(!PrefixMemory::default().learn(&cut));
}

#[test]
fn a_short_body_is_reported_as_it_is() {
    assert_eq!(error_body("{ \"error\": \"authz\" }"), "{ \"error\": \"authz\" }");
    let long = "x".repeat(500);
    assert_eq!(error_body(&long).chars().count(), 200, "no prefix clause: the cut stands");
}
