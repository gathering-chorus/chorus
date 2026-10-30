#!/usr/bin/env bash
# @domain: awareness
# @test-type: integration:prod — fires concurrent requests at the running pulse service (:3475)
# @needs: machine — the running pulse service and its /cws script
# #4807 AC4 — /pulse stays answering under the prompt hook's real load. Every role's prompt
# hook asks /pulse at once; before #4807 each request past the 30 s cards cache started its own
# /cws script, they piled up, and /pulse stopped answering (15 of 40 concurrent calls took 2-8 s+
# on 2026-10-30 08:32). Two bursts of 20, 32 s apart so the second lands after the cache expires.
# PASS: every call answers 200 within 1 s, and at most one /cws script runs at any moment.
set -uo pipefail
URL="${PULSE_URL:-http://127.0.0.1:3475/pulse}"
N="${BURST:-20}"; LIMIT="${LIMIT_S:-1.0}"; GAP="${GAP_S:-32}"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
# count only prod pulse's own /cws children: werk pulses and hand-run /cws are not this service
# (prod runs from the canonical checkout ".../chorus/platform/...", never ".../chorus-werk/<slot>/platform/...")
PULSE_PID="${PULSE_PID:-$(pgrep -f '/chorus/platform/pulse/dist/service\.js' | head -1)}"
[ -n "$PULSE_PID" ] || { echo "FAIL: no prod pulse process (node .../chorus/platform/pulse/dist/service.js)"; exit 1; }

burst() {
  local tag="$1" peak=0 i n
  for i in $(seq 1 "$N"); do
    curl -s -o /dev/null -m 10 -w '%{http_code} %{time_total}\n' "$URL" >"$tmp/$tag.$i" &
  done
  # sample the /cws children while the burst is in flight
  for i in $(seq 1 25); do
    n=$(pgrep -P "$PULSE_PID" -f 'chorus-werk-status' | wc -l | tr -d ' ')
    [ "$n" -gt "$peak" ] && peak=$n
    sleep 0.1
  done
  wait
  echo "$peak" >"$tmp/$tag.peak"
}

burst a; sleep "$GAP"; burst b
cat "$tmp"/a.[0-9]* "$tmp"/b.[0-9]* >"$tmp/all"
total=$(wc -l <"$tmp/all" | tr -d ' ')
bad=$(awk -v l="$LIMIT" '$1!="200" || $2+0>l' "$tmp/all" | wc -l | tr -d ' ')
max=$(awk '{print $2}' "$tmp/all" | sort -n | tail -1)
peak=$(cat "$tmp"/a.peak "$tmp"/b.peak | sort -n | tail -1)
echo "pulse_pid=$PULSE_PID calls=$total slow_or_failed=$bad max_s=$max cws_peak=$peak (limit ${LIMIT}s, peak ≤1)"
[ "$total" -eq $((N * 2)) ] || { echo "FAIL: only $total of $((N * 2)) calls reported"; exit 1; }
[ "$bad" -eq 0 ] || { echo "FAIL: $bad calls failed or took over ${LIMIT}s"; exit 1; }
[ "$peak" -le 1 ] || { echo "FAIL: $peak /cws scripts ran at once"; exit 1; }
echo PASS
