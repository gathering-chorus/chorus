#!/usr/bin/env bash
# @test-type: integration — reads the live session-sweep log
# @domain: identity
# @needs: machine — prod after the land
# @card: #4944 · owner: wren
# #4944 AC4 — in prod after the land, the session sweep reports failed=0 for 10 minutes.
# The sweep runs every 60s (com.chorus.session-sweep StartInterval) and its lines carry no
# timestamp, so "10 minutes" is the last 10 summary lines, and the log must have been written
# in the last 3 minutes (a dead sweep is RED, never a vacuous pass).
set -u
# the sweep runs as jeffbridwell (com.chorus.session-sweep), and so does the land that proves this
LOG="${SWEEP_LOG:-$HOME/Library/Logs/Gathering/session-sweep.log}"
N="${SWEEP_RUNS:-10}"
[ -r "$LOG" ] || { echo "RED: cannot read $LOG"; exit 1; }
age=$(( $(date +%s) - $(stat -f %m "$LOG") ))
[ "$age" -le 180 ] || { echo "RED: $LOG last written ${age}s ago; the sweep is not running"; exit 1; }
lines=$(grep '^sweep:' "$LOG" | tail -n "$N")
have=$(printf '%s\n' "$lines" | grep -c '^sweep:')
[ "$have" -ge "$N" ] || { echo "RED: only $have sweep runs in $LOG, need $N"; exit 1; }
bad=$(printf '%s\n' "$lines" | grep -v ', 0 failed$' || true)
if [ -n "$bad" ]; then
  echo "RED: $(printf '%s\n' "$bad" | wc -l | tr -d ' ') of the last $N sweeps failed:"
  printf '%s\n' "$bad" | tail -1
  grep -v '^sweep:' "$LOG" | tail -3
  exit 1
fi
echo "GREEN: the last $N sweeps all report 0 failed"
