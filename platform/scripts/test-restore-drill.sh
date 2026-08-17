#!/usr/bin/env bash
# @test-type: integration — the weekly restore drill's nightly-discovered wrapper.
#
# #3616. Auto-discovered by nightly-suites.sh (test-*.sh), but a FULL restore is
# ~25 minutes and ~16G of traffic — running it nightly would tax the box every
# night to re-answer a question that changes weekly. So:
#
#   • WEEKLY cadence: runs the real drill only if the last PASS is older than
#     RESTORE_DRILL_MAX_AGE_DAYS (default 7).
#   • DAILY honesty: on the off-days it still checks the AGE of the last verdict
#     and REDS if it is stale — a drill nobody notices stopped running is the
#     same blindness as no drill (the security-fitness ledger lesson, #3900).
#   • Verdict source is the SPINE (ops.restore.drill), not a local flag file:
#     the record that survives is the one the whole team can read.
set -uo pipefail
R="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SPINE="${CHORUS_SPINE:-$HOME/.chorus/chorus.log}"
MAX_AGE_DAYS="${RESTORE_DRILL_MAX_AGE_DAYS:-7}"
STALE_RED_DAYS="${RESTORE_DRILL_STALE_RED_DAYS:-10}"

last_pass_epoch() {
  # newest ops.restore.drill with verdict=pass; empty if never
  local line
  line="$(grep -a '"ops.restore.drill"' "$SPINE" 2>/dev/null | grep -a 'verdict=pass\|"verdict":"pass"' | tail -1)"
  [ -n "$line" ] || { echo ""; return; }
  local ts
  ts="$(printf '%s' "$line" | sed -E 's/.*"timestamp":"([^"]+)".*/\1/' | cut -c1-19)"
  date -j -f '%Y-%m-%dT%H:%M:%S' "$ts" '+%s' 2>/dev/null || echo ""
}

LAST="$(last_pass_epoch)"
NOW="$(date +%s)"
if [ -n "$LAST" ]; then
  AGE_DAYS=$(( (NOW - LAST) / 86400 ))
  echo "restore-drill: last PASS ${AGE_DAYS}d ago"
  if [ "$AGE_DAYS" -lt "$MAX_AGE_DAYS" ]; then
    echo "restore-drill: within the ${MAX_AGE_DAYS}d window — not re-running (weekly cadence)"
    exit 0
  fi
  if [ "$AGE_DAYS" -ge "$STALE_RED_DAYS" ]; then
    echo "restore-drill: RED — last proven restore is ${AGE_DAYS}d old (>= ${STALE_RED_DAYS}d)"
    # fall through and RUN it; a stale drill must be re-proven, not just reported
  fi
else
  echo "restore-drill: no PASS on the spine — the restore has never been proven"
fi

exec bash "$R/platform/scripts/restore-drill.sh"
