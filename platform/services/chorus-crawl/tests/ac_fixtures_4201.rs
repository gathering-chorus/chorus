//! #4201 AC3 and AC4 — the two negative proofs, run against fixture FILES on
//! disk rather than string literals, so what is proven is what a pass reads.
//!
//! The fixtures live here and not under `platform/tests/` on purpose: a file
//! there is a real test file and the crawl would register it, moving the very
//! counts these proofs are about. The path a rule sees is an argument, so each
//! fixture is fed the path its case is about.

use chorus_crawl::domain::{listing, place_in_file, Placement};

fn valid() -> Vec<String> {
    ["cards", "search", "services", "tests", "domains", "spine"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn fixture(name: &str) -> String {
    // #4167 — run time, not compile time. The nightly shares one cargo target
    // dir across every werk, so a binary compiled in werk A is reused in werk B
    // and a baked-in path points at a tree that may already be torn down. That
    // is the #4030 guard's whole subject, and this line was failing it.
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo at run time");
    let p = format!("{root}/tests/fixtures/{name}");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("fixture {p}: {e}"))
}

/// AC3. The route says `cards`, the header card says `search`. Two rules, two
/// domains, no majority — the file must come out untagged and the line must
/// name the file and BOTH domains, so a reader can settle it.
#[test]
fn ac3_route_says_cards_header_card_says_search_yields_one_conflict_line() {
    let content = fixture("route-cards-header-search.test.ts");
    let card_domain = |n: u32| (n == 4201).then(|| "search".to_string());
    let path = "platform/api/tests/route-cards-header-search.test.ts";

    // The header card is what makes this a conflict rather than a plain tag,
    // so the fixture must actually carry one the rule can read.
    let p = place_in_file(
        &content,
        path,
        None,
        &[],
        &[],
        &valid(), &card_domain, &|_| None);
    assert_eq!(p.domain(), None, "a file two rules read differently must not be tagged");

    let line = listing(path, &p).expect("a conflict prints a line");
    assert!(line.starts_with("conflict "), "{line}");
    assert!(line.contains(path), "the line must name the file: {line}");
    assert!(line.contains("cards"), "{line}");
    assert!(line.contains("search"), "{line}");
}

/// AC4. A card number and nothing else, at a path under `platform/tests/`.
/// The retired folder rule read a file like this as `services` because of
/// where it sat. Unplaced is the correct answer; `services` is the bug.
#[test]
fn ac4_a_card_number_under_platform_tests_is_unplaced_never_services() {
    let content = fixture("platform-tests-card-number-only.bats");
    // The card in the header resolves to NO domain — a card number alone is
    // not a signal. If it resolved, the card rule would legitimately tag it.
    let no_domain = |_: u32| None;
    // #4222 — the path was "4201-card-number-only.bats". That name contains the
    // word "card", and the file-name rule added on #4222 reads a file's own
    // name, so the fixture started resolving to `cards` — for a reason #4201
    // never intended to test. The AC here is that a CARD NUMBER alone is not a
    // signal and the folder never speaks; neither claim involves the word.
    // Renamed to a name that states nothing, so the fixture tests what it says.
    let path = "platform/tests/4201-number-only.bats";

    let p = place_in_file(
        &content,
        path,
        None,
        &[],
        &[],
        &valid(), &no_domain, &|_| None);
    assert_eq!(p, Placement::Unplaced, "got {p:?}");
    let line = listing(path, &p).expect("an unplaced file prints a line");
    assert_eq!(line, format!("unplaced {path}: no rule fired"));
    assert!(!line.contains("services"), "the folder must never speak: {line}");
}

/// CONTROL for AC4: the same file at the same path, when its card DOES resolve
/// to a domain, tags from the card — proving the unplaced above comes from the
/// absent signal and not from a blanket refusal on that folder.
#[test]
fn ac4_control_the_same_file_tags_when_its_card_has_a_domain() {
    let content = fixture("platform-tests-card-number-only.bats");
    let card_domain = |n: u32| (n == 4201).then(|| "tests".to_string());
    let p = place_in_file(
        &content,
        // same rename as above, same reason
        "platform/tests/4201-number-only.bats",
        None,
        &[],
        &[],
        &valid(),
        &card_domain,
        &|_| None,
    );
    assert_eq!(p.domain(), Some("tests"));
}
