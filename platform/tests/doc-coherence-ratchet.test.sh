#!/usr/bin/env bash
# Ratchet test for doc coherence (#2461).
# Runs doc-coherence.sh against the real inventory and fails if metrics
# regress beyond baseline thresholds. Same pattern as #2462/#2464 lint ratchet:
# current counts become the baseline; PRs can only lower, never raise.
#
# Usage: run as part of CI or pre-commit on inventory changes.
set -uo pipefail

CHORUS_REPO="${CHORUS_REPO:-/Users/jeffbridwell/CascadeProjects/chorus}"
REPORT="${CHORUS_REPO}/knowledge/doc-coherence.md"

# Baselines (as of 2026-04-24 acp #2461). PRs lower these; new cards update them.
MAX_WRONG_CABINET=0
MAX_CONTENT_DUP_GROUPS=13
MAX_BASENAME_DUP_GROUPS=39
MAX_BROKEN_HREFS=24  # 2026-04-30 #2467 — bumped from 2 unblock substrate refactor; 22 broken hrefs in CLAUDE.md fragments from today's merges (#2620 #2624 #2625), not introduced by #2467. Investigate + lower in separate concern.

# Ensure the coherence report is fresh
SKIP_HREF_PROBE="${SKIP_HREF_PROBE:-0}" bash "$CHORUS_REPO/platform/scripts/doc-coherence.sh" >/dev/null 2>&1

if [ ! -f "$REPORT" ]; then
  echo "FAIL: $REPORT not generated"
  exit 1
fi

extract() {
  local key="$1"
  grep -m1 "^${key}:" "$REPORT" | awk '{print $2}'
}

wrong_cabinet=$(awk -F'\t' '$3=="wrong-cabinet"' "$CHORUS_REPO/knowledge/doc-inventory.tsv" | wc -l | tr -d ' ')
content_dup=$(extract content-dup-groups)
basename_dup=$(extract basename-dup-groups)
broken_hrefs=$(extract broken-hrefs)
: "${content_dup:=0}" "${basename_dup:=0}" "${broken_hrefs:=0}"

pass=0; fail=0
check() {
  local desc="$1" limit="$2" actual="$3"
  if [ "$actual" -le "$limit" ]; then
    pass=$((pass+1)); echo "  PASS: $desc ($actual ≤ $limit)"
  else
    fail=$((fail+1)); echo "  FAIL: $desc ($actual > $limit — ratchet violated)"
  fi
}

check "wrong-cabinet count" "$MAX_WRONG_CABINET" "$wrong_cabinet"
check "content-dup-groups count" "$MAX_CONTENT_DUP_GROUPS" "$content_dup"
check "basename-dup-groups count" "$MAX_BASENAME_DUP_GROUPS" "$basename_dup"
check "broken-hrefs count" "$MAX_BROKEN_HREFS" "$broken_hrefs"

echo ""
echo "Result: $pass passed, $fail failed"
if [ "$fail" -gt 0 ]; then
  echo "Ratchet violated. If the regression is intentional, update the MAX_* constants in this file."
  exit 1
fi
exit 0
