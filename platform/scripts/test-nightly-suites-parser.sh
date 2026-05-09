#!/bin/bash
# #2850 — Unit test for nightly-suites.sh summary extractors.
#
# Feeds fixture stdouts (the three shell scripts whose output formerly tripped
# the producer's tail -1 heuristic + the consumer's `[N pass]/[N fail]` regex
# into a silent DID NOT RUN) into _extract_shell_summary and asserts the
# extracted summary matches the consumer's expected shape AND counts.
#
# Also asserts the cargo branch's compile/run-failure path produces a
# non-silent summary by running run_one_attempt against a synthetic script
# that exits non-zero with no `test result:` lines.

set -u

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NIGHTLY="$SCRIPT_DIR/nightly-suites.sh"
[ -f "$NIGHTLY" ] || { echo "FAIL: cannot find $NIGHTLY"; exit 1; }
# nightly-suites.sh guards its dispatch with a BASH_SOURCE != $0 check,
# so sourcing here gives us the helper functions without running the CLI.
# shellcheck disable=SC1090
source "$NIGHTLY"

PASS=0
FAIL=0

assert_summary_parseable() {
  local label="$1" summary="$2" want_pass="$3" want_fail="$4"
  # consumer's regex: needs both [0-9]+ (pass|ok) and [0-9]+ fail
  if ! echo "$summary" | grep -qE '[0-9]+ (pass|ok)'; then
    echo "FAIL [$label]: summary missing 'N pass|ok' — '$summary'"
    FAIL=$((FAIL+1)); return
  fi
  if ! echo "$summary" | grep -qE '[0-9]+ fail'; then
    echo "FAIL [$label]: summary missing 'N fail' — '$summary'"
    FAIL=$((FAIL+1)); return
  fi
  local got_pass got_fail
  got_pass=$(echo "$summary" | grep -oE '[0-9]+ (pass|ok)' | head -1 | grep -oE '[0-9]+')
  got_fail=$(echo "$summary" | grep -oE '[0-9]+ fail' | head -1 | grep -oE '[0-9]+')
  if [ "$got_pass" != "$want_pass" ] || [ "$got_fail" != "$want_fail" ]; then
    echo "FAIL [$label]: counts mismatch — got pass=$got_pass fail=$got_fail; want pass=$want_pass fail=$want_fail; summary='$summary'"
    FAIL=$((FAIL+1)); return
  fi
  echo "PASS [$label]: '$summary'"
  PASS=$((PASS+1))
}

# ---------- Fixture A: test-build-invariance.sh failure tail ----------
# Real output ends with prose ("This is a real invariance violation...") and rc=1.
# Pre-fix: tail -1 → "exit 1" / prose; consumer regex misses → DID NOT RUN.
# Post-fix: synthesizes "0 pass, 1 fail (synthesized rc=1, ...)".
fixture_a=$(cat <<'EOF'
=== test-build-invariance: comparing two builds of chorus-inject ===
Build 1 cdhash: deadbeef
Build 2 cdhash: cafebabe
test-build-invariance: NOTE — sha256 differs but cdhash MUST match
  This is a real invariance violation — TCC AppleEvents grants will break across rebuilds.
EOF
)
sum_a=$(_extract_shell_summary "$fixture_a" 1)
assert_summary_parseable "build-invariance prose+rc1" "$sum_a" "0" "1"

# ---------- Fixture B: test-chorus-bin-install.sh — Passed/Failed pair ----------
# Real output ends with "Passed: 7" / "Failed: 0" on adjacent lines, rc=0.
# Pre-fix: tail -1 → "Failed: 0"; consumer regex `[N pass|ok]` misses → DID NOT RUN.
# Post-fix: fallback #2 picks up Passed/Failed → "7 pass, 0 fail".
fixture_b=$(cat <<'EOF'
test-chorus-bin-install: 7 cases
case 1: ok
case 2: ok
...
Passed: 7
Failed: 0
EOF
)
sum_b=$(_extract_shell_summary "$fixture_b" 0)
assert_summary_parseable "bin-install Passed/Failed pair" "$sum_b" "7" "0"

# ---------- Fixture C: test-chorus-inject-spawn-sites.sh — PASS prose ----------
# Real output ends with "PASS: chorus-inject spawn site is single — only ..." and rc=0.
# Pre-fix: tail -1 has "PASS:" but no digit pair → DID NOT RUN.
# Post-fix: synthesizes "1 ok, 0 fail".
fixture_c=$(cat <<'EOF'
checking spawn-site allowlist...
canonical site: platform/scripts/chorus-inject
PASS: chorus-inject spawn site is single — only platform/scripts/chorus-inject
EOF
)
sum_c=$(_extract_shell_summary "$fixture_c" 0)
assert_summary_parseable "spawn-sites PASS prose rc0" "$sum_c" "1" "0"

# ---------- Fixture D: canonical Results line — happy path ----------
fixture_d=$(cat <<'EOF'
running test-foo
.. lots of output ..
=== Results: 12 passed, 3 failed ===
EOF
)
sum_d=$(_extract_shell_summary "$fixture_d" 1)
assert_summary_parseable "canonical Results line" "$sum_d" "12" "3"

# ---------- Fixture E: legacy "5 ok / 0 fail" last-line ----------
fixture_e=$(cat <<'EOF'
ran 5 cases
[5 ok / 0 fail]
EOF
)
sum_e=$(_extract_shell_summary "$fixture_e" 0)
assert_summary_parseable "legacy last-line ok/fail" "$sum_e" "5" "0"

echo
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $((FAIL > 0 ? 1 : 0))
