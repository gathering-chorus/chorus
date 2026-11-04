#!/usr/bin/env bash
# security-probes-run.sh (#3900) — execute every committed security probe.
#
# Jeff 2026-08-15: "security changes consistently overstated and under
# implemented" + "tracker cards get built and then decay." So: claims are
# PROBES that RUN. Commands live HERE, in reviewed committed files — never in
# the writable graph (a probeCommand field would be remote-code-as-data; the
# graph registers probes and the spine carries results, but the execution
# surface stays in git where land review sees it).
#
# Exit 0 = every probe green. Nonzero = at least one control is NOT what we
# claim; each red names its probe and prints its evidence.
# Spine: security.probe.result per probe {probe, green}; summary line at end.
#
# The ledger IS this run's output: /api/chorus/security-fitness serves the
# snapshot verbatim with its age. Before #3900 that file was a HAND-TAKEN
# snapshot (last one 5 days stale — the tracker-decay pattern Jeff named).
# Every run refreshes it; a stale ledger now means the runner stopped, and
# the endpoint's ageMinutes says so out loud.
set -uo pipefail
R="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
D="$R/platform/security/probes.d"
CHORUS_LOG="${CHORUS_LOG:-$R/platform/scripts/chorus-log}"
[ -d "$D" ] || { echo "security-probes: probes.d missing at $D"; exit 1; }
total=0; red=0; rows=""
for p in "$D"/*.sh; do
  [ -f "$p" ] || continue
  name="$(basename "$p" .sh)"
  claim="$(sed -n 's/^# GREEN = //p' "$p" | head -1 | sed 's/"/\\"/g')"
  total=$((total+1))
  if out=$(bash "$p" 2>&1); then
    verdict=green
    echo "GREEN $name"
    "$CHORUS_LOG" security.probe.result silas probe="$name" green=true 2>/dev/null || true
  else
    verdict=red
    red=$((red+1))
    echo "RED   $name"
    printf '%s\n' "$out" | sed 's/^/      /'
    "$CHORUS_LOG" security.probe.result silas probe="$name" green=false 2>/dev/null || true
  fi
  detail="$(printf '%s' "$out" | tail -1 | cut -c1-160 | sed 's/"/\\"/g')"
  [ -n "$rows" ] && rows="$rows,"
  rows="$rows
    {\"row\": $total, \"claim\": \"$name — ${claim:-committed probe}\", \"verdict\": \"$verdict\", \"detail\": \"${detail}\"}"
done
# A run that found no probes must never pass vacuously (#3734).
[ "$total" -ge 1 ] || { echo "security-probes: ZERO probes found — refusing vacuous green"; exit 1; }

SNAP="${FITNESS_SNAPSHOT:-$HOME/.chorus/security-fitness.json}"
printf '{\n  "taken": "%s (security-probes-run)",\n  "rows": [%s\n  ]\n}\n' \
  "$(TZ=America/New_York date '+%Y-%m-%d %H:%M %Z')" "$rows" > "$SNAP.tmp"
if command -v python3 >/dev/null 2>&1 && ! python3 -m json.tool "$SNAP.tmp" >/dev/null 2>&1; then
  echo "security-probes: snapshot did not serialize to valid JSON — ledger NOT replaced"
  rm -f "$SNAP.tmp"
else
  mv "$SNAP.tmp" "$SNAP"
fi

echo "security-probes: $((total-red))/$total green"
[ "$red" -eq 0 ]
