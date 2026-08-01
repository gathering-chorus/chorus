//! #3721 — RETIRED. This file tested `skills/demo/gates/preflight.sh`, which no
//! longer exists: Wren's #3046 ("demo v2: werk-demo crate + retire v1
//! gates/hooks") deleted the v1 gate scripts and moved demo preflight into the
//! werk-demo crate. The tests outlived the mechanism by design-drift, not intent.
//!
//! Both tests were broken, in the two different ways this card keeps finding:
//!
//!   - `preflight_passes_with_path` FAILED locally with "No such file or
//!     directory". It was invisible on CI because it needs RUN_INTEGRATION *and*
//!     a live Vikunja WIP card; CI has the first and not the second, so it took
//!     the "no WIP card available" early return and reported success. Broken in
//!     one environment, skipped in the other — Wren's line for this shape is that
//!     it isn't a test, it's decoration that reads green.
//!
//!   - `preflight_fails_without_path` PASSED, for entirely the wrong reason. It
//!     asserted the script FAILS when PATH is stripped. A script that does not
//!     exist also fails, so it has been green on a deleted file since #3046 —
//!     proving nothing about PATH at all.
//!
//! Not replaced here: the concern (a spawned gate needs PATH to reach the cards
//! CLI, whose absence caused 31 consecutive false demo denials) belongs to
//! whatever spawns the gate, and that is now werk-demo — see
//! platform/services/werk-demo/tests/e2e.rs. Re-adding a chorus-hooks test for a
//! script chorus-hooks no longer owns or invokes would recreate exactly the
//! orphaning that produced this file.
