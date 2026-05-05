#!/usr/bin/env bash
# test-chorus-bin-install.sh — tests for chorus-bin-install (#2734).
#
# chorus-bin-install takes a freshly built+signed binary and installs it to
# ~/.chorus/bin/<name> with atomic move, emitting binary.deployed on the
# spine. Tests run against a temp $HOME so nothing touches the real
# ~/.chorus/bin or spine.
#
# Run directly (not via Claude hook-intercepted Bash).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INSTALL="$SCRIPT_DIR/chorus-bin-install"

PASS=0
FAIL=0

if [ ! -x "$INSTALL" ]; then
  echo "FAIL: chorus-bin-install not found or not executable at $INSTALL"
  exit 1
fi

TEST_HOME=$(mktemp -d)
SPINE_LOG="$TEST_HOME/spine.log"

# A fake "binary" — just an executable file. The install script doesn't
# verify signature here (build-signed.sh does that before calling install);
# install only handles atomic move + spine emit.
SRC=$(mktemp)
echo "fake binary v1" > "$SRC"
chmod +x "$SRC"

cleanup() {
  rm -rf "$TEST_HOME"
  rm -f "$SRC"
}
trap cleanup EXIT

export HOME="$TEST_HOME"
export CHORUS_BIN_SPINE_LOG="$SPINE_LOG"  # test override for spine emit

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

# `echo X | grep` doesn't pass through assert cleanly — use a helper.
contains() { echo "$1" | grep -q "$2"; }

# --- TEST 1: install creates ~/.chorus/bin/ if missing ---
assert "bin dir does not exist before install" test ! -d "$HOME/.chorus/bin"
"$INSTALL" "$SRC" chorus-test-binary > /dev/null 2>&1
assert "install creates bin dir" test -d "$HOME/.chorus/bin"
assert "install places binary at expected name" test -f "$HOME/.chorus/bin/chorus-test-binary"
assert "installed binary is executable" test -x "$HOME/.chorus/bin/chorus-test-binary"

# --- TEST 2: install preserves binary content ---
INSTALLED_HASH=$(shasum -a 256 "$HOME/.chorus/bin/chorus-test-binary" | awk '{print $1}')
SRC_HASH=$(shasum -a 256 "$SRC" | awk '{print $1}')
assert "installed binary content matches source" test "$INSTALLED_HASH" = "$SRC_HASH"

# --- TEST 3: install is atomic — no partial state if move fails ---
# Verified indirectly: the script uses mv, which is atomic on the same
# filesystem. We assert the install succeeds without leaving a temp file.
TMP_FILES=$(find "$HOME/.chorus/bin/" -name "*.tmp" -o -name "*.partial" 2>/dev/null)
assert "no temp/partial files after install" test -z "$TMP_FILES"

# --- TEST 4: install emits binary.deployed spine event ---
assert "spine log was written" test -f "$SPINE_LOG"
SPINE_CONTENT=$(cat "$SPINE_LOG" 2>/dev/null || echo "")
assert "spine has binary.deployed event" contains "$SPINE_CONTENT" "binary.deployed"
assert "spine event names the binary" contains "$SPINE_CONTENT" "binary=chorus-test-binary"

# --- TEST 5: re-install of same source overwrites cleanly ---
echo "fake binary v2" > "$SRC"
chmod +x "$SRC"
"$INSTALL" "$SRC" chorus-test-binary > /dev/null 2>&1
NEW_CONTENT=$(cat "$HOME/.chorus/bin/chorus-test-binary")
assert "re-install overwrites the binary" test "$NEW_CONTENT" = "fake binary v2"

# --- TEST 6: identical source re-installed twice produces identical content ---
echo "stable v1" > "$SRC"
chmod +x "$SRC"
"$INSTALL" "$SRC" chorus-test-binary > /dev/null 2>&1
H1=$(shasum -a 256 "$HOME/.chorus/bin/chorus-test-binary" | awk '{print $1}')
"$INSTALL" "$SRC" chorus-test-binary > /dev/null 2>&1
H2=$(shasum -a 256 "$HOME/.chorus/bin/chorus-test-binary" | awk '{print $1}')
assert "identical source = identical installed hash" test "$H1" = "$H2"

# --- TEST 7: install refuses missing source ---
"$INSTALL" /nonexistent/path chorus-test > /dev/null 2>&1
RC=$?
assert "install refuses nonexistent source" test "$RC" -ne 0

# --- TEST 8: install refuses missing args ---
"$INSTALL" > /dev/null 2>&1
RC=$?
assert "install refuses no args" test "$RC" -ne 0

"$INSTALL" "$SRC" > /dev/null 2>&1
RC=$?
assert "install refuses missing binary name" test "$RC" -ne 0

echo "---"
echo "Passed: $PASS"
echo "Failed: $FAIL"
[ "$FAIL" -eq 0 ]
