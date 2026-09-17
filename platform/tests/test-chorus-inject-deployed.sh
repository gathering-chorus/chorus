#!/usr/bin/env bash
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
# invocation and names the one supported path (the chorus_nudge_message MCP
# tool). The Rust binary is the pulse worker's delivery primitive and is no
# longer what lives at this path, so a codesign/Usage assertion here asserted
# a retired product (nightly red 2026-09-17, read on #4196). What Jeff needs
# from this path: it exists, it runs, and it points a caller at the real door.
if [ -x "$BIN" ]; then
  OUT=$("$BIN" 2>&1 || true)
  assert "chorus-inject refuses direct invocation (not-canonical-caller)" \
    grep -q "not-canonical-caller" <<< "$OUT"
  assert "chorus-inject names the supported path (chorus_nudge_message)" \
    grep -q "chorus_nudge_message" <<< "$OUT"
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
