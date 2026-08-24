#!/usr/bin/env bash
# GREEN = the recovery path carries no identity gate (the #3785 invariant, run
# as a probe so its decay would be VISIBLE nightly, not archaeological).
# #3999: red must carry its REASON — the old >/dev/null red said nothing, which
# is itself the empty-reason probe defect. Print the failing bats lines.
R="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
OUT="$(bats "$R/platform/tests/recovery-path-ungated-3785.bats" 2>&1)" && exit 0
echo "$OUT" | grep -E '^not ok|# ' | head -6
exit 1
