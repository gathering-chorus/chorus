#!/usr/bin/env bash
# test-no-quick-flag.sh — regression guard for #3293.
#
# #3293 removed the `--quick`/`-q` escape hatch from `cards add`: every card now
# carries the Experience+AC floor (no substance-free cards). This guard fails if
# the flag is reintroduced anywhere a card is actually filed — the functional
# forms are a quoted '--quick' / "--quick" literal (code argv) or a `cards add
# ... --quick` command (scripts/skills). Comments that NARRATE the removal
# ("#3293 removed --quick") are not functional and don't match these patterns.
# `git fetch -q` is git's quiet flag, unrelated — not matched (we only guard the
# long form). Test files are exempt (they assert the flag's ABSENCE).
#
# Run directly (not via Claude hook-intercepted Bash).

set -uo pipefail

trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: 1 passed, 0 failed ==="; else echo "=== Results: 0 passed, 1 failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"

EXCLUDE_PATTERN='node_modules/|target/|\.consumed$|/tests/|test-|_test\.|\.test\.|\.bats$|test-no-quick-flag\.sh'

count_quick() {
  cd "$REPO_ROOT"
  # Functional reintroduction vectors only: a quoted flag literal in code, or a
  # `cards add ... --quick` invocation in a script/skill.
  # #3556 — git grep (tracked only), not grep -r (whole working tree). With
  # BASELINE=0 any untracked stray containing --quick (a brief, a /tmp copy, a
  # backup) flipped this red headless though committed code was clean. Hermetic now.
  git grep -nE "['\"]--quick['\"]|cards[[:space:]]+add[^\n]*--quick" 2>/dev/null \
    | grep -vE "$EXCLUDE_PATTERN" \
    | wc -l \
    | tr -d ' '
}

BASELINE=0
current=$(count_quick)

if [ "$current" -gt "$BASELINE" ]; then
  echo "FAIL: functional --quick usage reintroduced (count: $current, baseline: $BASELINE)"
  echo "Offenders:"
  cd "$REPO_ROOT"
  git grep -nE "['\"]--quick['\"]|cards[[:space:]]+add[^\n]*--quick" 2>/dev/null \
    | grep -vE "$EXCLUDE_PATTERN" \
    | head -30
  echo "#3293: cards carry the Experience+AC floor — there is no --quick bypass."
  exit 1
fi

echo "PASS: no functional --quick usage (baseline $BASELINE)"
exit 0
