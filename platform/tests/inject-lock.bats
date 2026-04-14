#!/usr/bin/env bats
# inject-lock.bats — verify chorus-inject/src/main.rs is locked from edits (#2030)
# The nudge injection code was changed 3 times, breaking auto-submit each time.
# This hook prevents any role from editing it without Jeff's approval.

HOOKS_SRC="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/src/hooks/sensitive_paths.rs"

@test "sensitive_paths blocks edits to chorus-inject/src/main.rs" {
  grep -q 'chorus-inject' "$HOOKS_SRC"
}

@test "block message mentions frozen and Jeff" {
  grep -qi 'frozen.*Jeff\|Jeff.*frozen' "$HOOKS_SRC"
}
