//! #4033 — result names must be unique across the units of one run.
//! Negative proof (#3734): the old scheme (every unit numbers from 0 under the
//! run's shared ts) is built for real and shown to collide; the fix (each unit
//! claims a base from the run counter) is shown to produce disjoint names.

use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;
use werk_test::{claim_index_base, test_result_name, test_result_payload};

const TS: u128 = 1_788_112_000_000;

fn names(base: usize, n: usize) -> Vec<String> {
    (0..n).map(|i| test_result_name("nightly", TS, base + i)).collect()
}

#[test]
fn negative_proof_units_numbered_from_zero_collide() {
    // unit one: 5 cases, unit two: 176 cases — the 12:45 shape (athena-deploy,
    // then athena-model). Both from 0, same ts: the first five names are the same.
    let one = names(0, 5);
    let two = names(0, 176);
    let overlap: Vec<_> = one.iter().filter(|n| two.contains(n)).collect();
    assert_eq!(overlap.len(), 5, "the old scheme must be shown to collide");
}

#[test]
fn units_that_claim_a_base_mint_disjoint_names() {
    let counter = AtomicUsize::new(0);
    let b1 = claim_index_base(&counter, 5);
    let b2 = claim_index_base(&counter, 176);
    let b3 = claim_index_base(&counter, 92);
    assert_eq!((b1, b2, b3), (0, 5, 181));
    let all: Vec<String> = [names(b1, 5), names(b2, 176), names(b3, 92)].concat();
    let unique: HashSet<&String> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "every name in the run is unique");
}

#[test]
fn the_payload_carries_the_claimed_index_in_its_name() {
    let p = test_result_payload("platform/tests/x.bats", "t1", "pass", "test-x", "nightly", "silas", "tr", TS, 181);
    assert!(p.contains(&format!("\"name\":\"{}\"", test_result_name("nightly", TS, 181))), "{p}");
}
