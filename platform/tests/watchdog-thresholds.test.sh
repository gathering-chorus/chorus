#!/usr/bin/env bash
# watchdog-thresholds.test.sh — #3446 AC2 (red-first, DEC-1674)
#
# watchdog.sh runs `set -u`. Any *_THRESHOLD variable it references but never
# defines aborts the script at runtime (unbound variable) — exactly the latent
# bug the dead ProgramArguments path masked since 2026-04-14. Guard the whole
# class: every _THRESHOLD variable used must be defined in the script.
#
# Run: bash platform/tests/watchdog-thresholds.test.sh

set -uo pipefail

PASS=0
FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }

echo "=== watchdog threshold variables are all defined (#3446 AC2) ==="

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WD="$REPO_ROOT/platform/scripts/watchdog.sh"

if [ ! -f "$WD" ]; then
  test_fail "watchdog.sh missing at platform/scripts/watchdog.sh"
  echo ""; echo "=== Results: $PASS passed, $FAIL failed ==="; exit 1
fi

# Variables referenced as $NAME_THRESHOLD or ${NAME_THRESHOLD...}
used=$(grep -oE '\$\{?[A-Z_]+_THRESHOLD' "$WD" | sed -E 's/^\$\{?//' | sort -u)

missing=""
for v in $used; do
  if ! grep -qE "^[[:space:]]*${v}=" "$WD"; then
    missing="$missing $v"
  fi
done

if [ -z "$missing" ]; then
  test_pass "all referenced _THRESHOLD variables are defined ($(echo "$used" | tr '\n' ' '))"
else
  test_fail "undefined _THRESHOLD variable(s) referenced under set -u:$missing"
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
