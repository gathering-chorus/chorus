#!/usr/bin/env bash
# Ratchet test for doc coherence (#2461).
# Runs doc-coherence.sh against the real inventory and fails if metrics
# regress beyond baseline thresholds. Same pattern as #2462/#2464 lint ratchet:
# current counts become the baseline; PRs can only lower, never raise.
#
# Usage: run as part of CI or pre-commit on inventory changes.
set -uo pipefail

# #2994: default CHORUS_REPO to the werk root that contains this test script
# (resolves via $(dirname "$BASH_SOURCE")/../..), not hardcoded canonical.
# Pre-commit running in a werk now exercises the werk's doc-coherence.sh —
# including the #2994 side-port autodetect — rather than the canonical
# version that's pre-this-card. Override via env for non-werk invocations.
CHORUS_REPO="${CHORUS_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
REPORT="${CHORUS_REPO}/knowledge/doc-coherence.md"

# Baselines (as of 2026-04-24 acp #2461). PRs lower these; new cards update them.
MAX_WRONG_CABINET=0
MAX_CONTENT_DUP_GROUPS=13
MAX_BASENAME_DUP_GROUPS=39
MAX_BROKEN_HREFS=1  # 2026-05-18 #2994 — lowered from 20 after fixing chorus-api routing (added 6 static mounts in server.ts for /skills, /diagrams, /roles/silas/docs, /roles/wren/{artifacts,docs,decisions}); 67 of the previous 68 broken-hrefs were routing gaps, not stale catalog entries. The remaining 1 is the last genuine stale entry. doc-coherence.sh now also auto-detects a side-port chorus-api on :3345 when CHORUS_API_HOST is unset (#2994), so the ratchet can see werk-dist routes before the deploy lands. Prior: 2026-05-06 #2752 cheat-bump 19→20 to unblock chorus_acp; 2026-05-03 #2704 lowered from 24 after /designing/{decisions,claudemd,domain-context,schemas} mounts landed.

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
