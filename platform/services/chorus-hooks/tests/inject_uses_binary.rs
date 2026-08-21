//! #3949 — RETIRED with its twin (tests/inject_binary_test.rs), same reason:
//! asserted a RELEASE binary exists at target/release — ambient build-state a
//! fresh werk can never satisfy (#3528). Run 52 "ghost binary" was actually
//! partly THIS sibling, alive under a different file name — the sweep that
//! retired the first file missed it because the assertion was duplicated
//! across two files (which is its own lesson). Deploy-state lives in
//! chorus-health's deployed-equals-running check against ~/.chorus/bin.
