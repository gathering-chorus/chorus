#!/bin/bash
# test-role-state-spine.sh — role-state under #4028: nothing declared, nothing stored.
#
# Was (#1945): every transition emits role.state.changed. Now: the only thing a
# role says is `blocked`, as a JSON spine event with a detail; every other
# state is derived on read by chorus-api and the CLI writes NOTHING for it.
# Brings its own world (#3528): CHORUS_LOG_FILE points at a temp spine unless
# the caller set one (the nightly does, #4065).
set -uo pipefail
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
PASS=0
FAIL=0

if [ -z "${CHORUS_LOG_FILE:-}" ]; then
  _tmp=$(mktemp -d "${TMPDIR:-/tmp}/role-state-spine.XXXXXX")
  export CHORUS_LOG_FILE="$_tmp/chorus.log"
  trap 'rm -rf "$_tmp"' EXIT
fi
CHORUS_LOG="$CHORUS_LOG_FILE"
touch "$CHORUS_LOG" 2>/dev/null || true
ROLE_STATE="${CHORUS_ROOT}/platform/scripts/role-state"

echo "=== role-state (#4028 — derived, blocked-as-event) ==="
echo ""

# --- Test 1: blocked emits ONE JSON role.blocked line with the detail ---
echo "Test 1: blocked emits role.blocked with detail"
BEFORE=$(wc -l < "$CHORUS_LOG" | tr -d ' ')
"$ROLE_STATE" silas blocked 'detail="waiting on the DAL cred"' >/dev/null 2>&1
NEW=$(tail -n +$((BEFORE + 1)) "$CHORUS_LOG")
if echo "$NEW" | python3 -c '
import sys, json
lines = [l for l in sys.stdin.read().splitlines() if l.strip()]
ok = len(lines) == 1
if ok:
    d = json.loads(lines[0])
    ok = d.get("event") == "role.blocked" and d.get("role") == "silas" and d.get("detail") == "waiting on the DAL cred"
sys.exit(0 if ok else 1)'; then
  echo "  PASS: one JSON role.blocked line, role=silas, detail carried"
  PASS=$((PASS+1))
else
  echo "  FAIL: expected exactly one JSON role.blocked line; got: $NEW"
  FAIL=$((FAIL+1))
fi

# --- Test 2: a derived state declares NOTHING (no spine line, no file) ---
echo "Test 2: 'building' writes no spine line and no declared file"
BEFORE=$(wc -l < "$CHORUS_LOG" | tr -d ' ')
LEGACY="/tmp/claude-team-scan/silas-declared.json"
LEGACY_MTIME_BEFORE=$(stat -f %m "$LEGACY" 2>/dev/null || echo none)
"$ROLE_STATE" silas building >/dev/null 2>&1
RC=$?
AFTER=$(wc -l < "$CHORUS_LOG" | tr -d ' ')
LEGACY_MTIME_AFTER=$(stat -f %m "$LEGACY" 2>/dev/null || echo none)
if [ "$RC" -eq 0 ] && [ "$BEFORE" -eq "$AFTER" ] && [ "$LEGACY_MTIME_BEFORE" = "$LEGACY_MTIME_AFTER" ]; then
  echo "  PASS: exit 0, spine unchanged, no declared file written"
  PASS=$((PASS+1))
else
  echo "  FAIL: rc=$RC lines $BEFORE→$AFTER legacy mtime $LEGACY_MTIME_BEFORE→$LEGACY_MTIME_AFTER"
  FAIL=$((FAIL+1))
fi

# --- Test 3 (negative proof, #3734): card= is still refused ---
echo "Test 3: card= is refused (board owns the card)"
"$ROLE_STATE" silas blocked card=4058 >/dev/null 2>&1
if [ $? -eq 2 ]; then
  echo "  PASS: exit 2 on card="
  PASS=$((PASS+1))
else
  echo "  FAIL: card= was not refused"
  FAIL=$((FAIL+1))
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
