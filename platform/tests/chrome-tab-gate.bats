#!/usr/bin/env bats
# chrome-tab-gate.bats — verify hook blocks role-initiated 'open http' (#1775)
# Prior work: DEC-090 established Chrome window separation. No enforcement hook existed.
# Current state: roles use 'open' which targets Jeff's Chrome window.
# Approach: PreToolUse hook on Bash, check for 'open http', block with redirect to chrome-window.sh.

HOOKS_SRC="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/src/hooks"

@test "chrome tab gate hook exists" {
  ls "$HOOKS_SRC"/chrome_tab_gate.rs
}

@test "hook blocks 'open http' pattern" {
  grep -q 'open.*http' "$HOOKS_SRC/chrome_tab_gate.rs"
}

@test "hook message references chrome-window.sh" {
  grep -q 'chrome-window' "$HOOKS_SRC/chrome_tab_gate.rs"
}
