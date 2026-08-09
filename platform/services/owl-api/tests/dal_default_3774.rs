//! #3774 AC1 — owl-api's write delegation targets the LIVE DAL binary.
//!
//! #3718 retired `chorus-model` into a fail-loud stub (`athena-model` is the
//! live binary, same verbs). owl-api's dal_run default was never repointed, so
//! every doored write since #3718 hit the stub: POST /tests returned the
//! retirement message, werk-test's wire-back failed 1208/1208 (HTTP 502), and
//! the tests domain froze at Aug 3. This pins the default so a future rename
//! without a repoint goes RED here instead of silently killing ingest again.
//!
//! Both assertions live in ONE test fn: they mutate process env, and separate
//! #[test] fns share the process (parallel threads) — a split would race.

#[test]
fn dal_default_is_the_live_binary_not_the_retired_stub() {
    // Default (env unset): must be athena-model — chorus-model is the #3718
    // fail-loud stub; pointing at it kills every write path in this crate.
    std::env::remove_var("CHORUS_MODEL_BIN");
    assert_eq!(
        owl_api::dal_bin(),
        "athena-model",
        "dal default must be the live DAL (athena-model); chorus-model was retired by #3718"
    );

    // Override respected: hermetic suites wire their stub DALs through this.
    std::env::set_var("CHORUS_MODEL_BIN", "/tmp/some-stub");
    assert_eq!(owl_api::dal_bin(), "/tmp/some-stub");
    std::env::remove_var("CHORUS_MODEL_BIN");
}
