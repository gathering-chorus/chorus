#!/usr/bin/env bash
# @domain: messages — picked by its owner, Kade, from the crawler's candidates (#4538)
# test-chorus-inject-deployed.sh — regression guard for #2769.
#
# CLAUDE.md and #2734 both say chorus-inject deploys to ~/.chorus/bin/
# alongside chorus-hook-shim and chorus-hooks. Until #2769, the actual
# deploy step had never been run — the binary lived only at
# target/release/chorus-inject, where the cdhash churns on every cargo
# build and TCC silently revokes AppleEvents permission.
#
# This test asserts the deploy is in place. If it fails after a future
# change, run: bash platform/scripts/build-signed.sh chorus-inject
#
# Run directly (not via Claude hook-intercepted Bash).
set -uo pipefail

BIN="$HOME/.chorus/bin/chorus-inject"
PASS=0
FAIL=0

assert() {
  local label="$1"; shift
  if "$@"; then
    PASS=$((PASS + 1))
    echo "PASS: $label"
  else
    FAIL=$((FAIL + 1))
    echo "FAIL: $label"
  fi
}

assert "chorus-inject exists at ~/.chorus/bin/" test -x "$BIN"
assert "chorus-inject is executable" test -f "$BIN"

# #2804 retired chorus-inject as a direct primitive: since 2026-08-29 the
# deploy artifact at ~/.chorus/bin/ is a small shell stub that refuses direct
# invocation and names the one supported path (the clearing-nudge MCP
# tool). The Rust binary is the pulse worker's delivery primitive and is no
# longer what lives at this path, so a codesign/Usage assertion here asserted
# a retired product (nightly red 2026-09-17, read on #4196). What Jeff needs
# from this path: it exists, it runs, and it points a caller at the real door.
#
# #4515 — the banner is the CODE's claim, so it is read from the binary this
# tree builds (#3701: measure the tree under test), not from ~/.chorus/bin,
# which holds main's build until the card lands. Measured against the installed
# binary, a card that renames the supported path is red before it can land and
# a card that breaks the banner is green until after. The two asserts above
# stay on the installed path: they are the deploy guard (#2769).
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TREE_BIN="$ROOT/platform/services/chorus-inject/target/release/chorus-inject"
if [ -x "$TREE_BIN" ]; then
  UNDER_TEST="$TREE_BIN"
else
  UNDER_TEST="$BIN"
  echo "NOTE: $TREE_BIN not built — the banner is read from the installed binary"
fi
if [ -x "$UNDER_TEST" ]; then
  OUT=$(env -u _NUDGE_PULSE_INTERNAL "$UNDER_TEST" 2>&1 || true)
  assert "chorus-inject refuses direct invocation (not-canonical-caller)" \
    grep -q "not-canonical-caller" <<< "$OUT"
  assert "chorus-inject names the supported path (clearing-nudge)" \
    grep -q "clearing-nudge" <<< "$OUT"
  # NEGATIVE: a banner-less stub (empty output) must fail both asserts
  EMPTY=""
  if grep -q "not-canonical-caller" <<< "$EMPTY"; then
    FAIL=$((FAIL + 1)); echo "FAIL: negative proof — an empty stub passed the refusal assert"
  else
    PASS=$((PASS + 1)); echo "PASS: negative proof — an empty stub fails the refusal assert"
  fi
fi

echo "---"
echo "Passed: $PASS"
echo "Failed: $FAIL"
[ "$FAIL" -eq 0 ]
