#!/usr/bin/env bash
# GREEN = the recovery path carries no identity gate (the #3785 invariant, run
# as a probe so its decay would be VISIBLE nightly, not archaeological).
R="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
bats "$R/platform/tests/recovery-path-ungated-3785.bats" >/dev/null 2>&1
