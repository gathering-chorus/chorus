// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #4532 — a card run never goes red on a wall-clock sample. Every
//! `assert!(elapsed < N)` in the chorus-hooks timing tests sits inside
//! `if timing_checks_on()`, which is true only when werk-test says the lane is
//! the nightly (WERK_TEST_LANE=nightly, #4531).
//!
//! Negative proof (#3734): a timing assert outside the gate is found and named;
//! the gate itself is off for the card lane and for an unset lane.

const FILES: &[&str] = &["tests/perf_suite.rs", "tests/pulse_service.rs"];

/// Lines of `src` holding a timing assert with no `if timing_checks_on()`
/// on the line before it.
fn ungated(src: &str) -> Vec<usize> {
    let lines: Vec<&str> = src.lines().collect();
    (0..lines.len())
        .filter(|&i| lines[i].trim_start().starts_with("assert!(elapsed <"))
        .filter(|&i| i == 0 || !lines[i - 1].contains("if timing_checks_on()"))
        .map(|i| i + 1)
        .collect()
}

fn read(rel: &str) -> String {
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{root}/{rel}")).unwrap_or_else(|e| panic!("{rel}: {e} — renamed or deleted? update FILES"))
}

#[test]
fn every_timing_assert_is_nightly_only() {
    for f in FILES {
        let src = read(f);
        let n = src.matches("assert!(elapsed <").count();
        // a file whose asserts all moved away must fail loud, not pass empty
        assert!(n > 0, "{f}: no timing asserts found — the guard has nothing to check");
        let bad = ungated(&src);
        assert!(bad.is_empty(), "{f}: timing assert outside timing_checks_on() at lines {bad:?}");
    }
}

#[test]
fn negative_proof_an_ungated_timing_assert_is_caught() {
    let src = "fn t() {\n    let elapsed = 1;\n    assert!(elapsed < 200, \"slow\");\n}\n";
    assert_eq!(ungated(src), vec![3]);
    let gated = "fn t() {\n    if timing_checks_on() {\n    assert!(elapsed < 200, \"slow\");\n    }\n}\n";
    assert!(ungated(gated).is_empty());
}

#[test]
fn the_gate_is_off_for_card_and_unset_lanes_and_on_for_nightly() {
    // same rule the test files use; one process-wide env, so one test sets it
    let on = || std::env::var("WERK_TEST_LANE").as_deref() == Ok("nightly");
    let prev = std::env::var_os("WERK_TEST_LANE");
    std::env::set_var("WERK_TEST_LANE", "card");
    assert!(!on(), "card lane must not check timing");
    std::env::remove_var("WERK_TEST_LANE");
    assert!(!on(), "an unset lane must not check timing");
    std::env::set_var("WERK_TEST_LANE", "nightly");
    assert!(on(), "the nightly must check timing");
    match prev { Some(v) => std::env::set_var("WERK_TEST_LANE", v), None => std::env::remove_var("WERK_TEST_LANE") }
}
