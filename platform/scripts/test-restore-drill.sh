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

# #4043 — backup FRESHNESS is its own daily red, separate from drill cadence:
# a backup agent that stops producing must page within 2 days, not wait for the
# weekly drill. Reads the newest ops.backup.fuseki.completed from the spine.
backup_fresh_check() { # $1=spine file, $2=now epoch, $3=max age hours → 0 fresh, 1 stale, 0 if never
  local spine="$1" now="$2" max_h="$3" line ts ep
  line="$(grep -a '"ops.backup.fuseki.completed"' "$spine" 2>/dev/null | tail -1)"
  [ -n "$line" ] || return 0   # never backed up on this box — drill's no-backup path owns that
  ts="$(printf '%s' "$line" | sed -E 's/.*"timestamp":"([^"]+)".*/\1/' | cut -c1-19)"
  ep="$(date -j -f '%Y-%m-%dT%H:%M:%S' "$ts" '+%s' 2>/dev/null || echo 0)"
  [ "${ep:-0}" -gt 0 ] || return 0
  [ $(( (now - ep) / 3600 )) -le "$max_h" ]
}

# Sourceable for tests (#3528): functions above, work below.
if [ "${BASH_SOURCE[0]}" != "$0" ]; then return 0 2>/dev/null || true; fi

if ! backup_fresh_check "$SPINE" "$(date +%s)" "${FUSEKI_BACKUP_MAX_AGE_HOURS:-48}"; then
  echo "restore-drill: RED — newest fuseki backup on the spine is older than ${FUSEKI_BACKUP_MAX_AGE_HOURS:-48}h (backup agent stopped producing)"
  exit 1
fi

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

# #4004 — translate the drill's UNMEASURABLE into the nightly's refusal code.
# restore-drill.sh exits 2 when it declines to measure — bedroom unreachable, no
# backup present, no scratch space, load over 12. That is "I could not look", not
# "the restore is broken", but the nightly scored it as a plain fail: on the
# 2026-08-25 run it went red purely because the box was at load 13.2. rc=3 is the
# nightly's SELF-REFUSED verdict (0 pass, 0 fail), which is what an unmeasurable
# drill honestly is. Every other exit passes through untouched, so a genuinely
# failed restore still goes red. #3616 built the age/cadence logic above; this
# only fixes how a refusal is reported.
bash "$R/platform/scripts/restore-drill.sh"
rc=$?
if [ "$rc" -eq 2 ]; then
  echo "restore-drill: UNMEASURABLE — declining rather than claiming a restore failure (rc=3, SELF-REFUSED)"
  exit 3
fi
exit "$rc"
