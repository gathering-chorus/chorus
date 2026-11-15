#!/usr/bin/env bats
# @test-type: unit — drives the real ops-nudge with a stub shim and a file:// door; reads the land plan
# @domain: alerts
# @card: #4977 · owner: silas
# Checks platform/scripts/ops-nudge, platform/scripts/land-4977-rows and platform/scripts/deep-health.sh.
#
# #4977 AC5 — the three with no Grafana signal stay quiet, named, until a sensor
# publishes their reading: messages-not-kept (needs messages-sent and messages-kept
# metrics), lancedb-stale (needs a lance folder age gauge), deep-health stale logs
# (needs a log age gauge). The first two are Alert rows set notify off by #4970 and
# untouched here; the third reaches roles as an alert case, never a nudge (#4957).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"; T="$BATS_TEST_TMPDIR"
  printf '#!/bin/sh\necho "$@" >> "%s/shim.log"\n' "$T" > "$T/shim"; chmod +x "$T/shim"
  mkdir -p "$T/door"
  for a in messages-not-kept lancedb-stale; do printf '{"data":{"notify":"off"}}' > "$T/door/$a"; done
  unset CHORUS_ROLE
}
send() { env CHORUS_HOOK_SHIM="$T/shim" ALERTS_DOOR="file://$T/door" bash "$ROOT/platform/scripts/ops-nudge" silas "$1"; }

@test "messages-not-kept and lancedb-stale, notify off, tell no one and are recorded" {
  run send "messages-not-kept: no-reading:messages-sent-2026-11-13"; [ "$status" -eq 0 ] || return 1
  run send "lancedb-stale: dir=/x age=5201h"; [ "$status" -eq 0 ] || return 1
  ! grep -q 'nudge-send' "$T/shim.log" || return 1
  [ "$(grep -c 'alert.quiet' "$T/shim.log")" -eq 2 ] || return 1
}
@test "this land does not turn them back on" {
  ! grep -qE '"(messages-not-kept|lancedb-stale)"' "$ROOT/platform/scripts/land-4977-rows" || return 1
}
@test "deep-health's stale logs reach roles as alert cases, not nudges" {
  D="$ROOT/platform/scripts/deep-health.sh"
  grep -q '"$ALERT_CASE" open deep-health' "$D" || return 1
  grep -q 'elif \[ ! -x "$ALERT_CASE" \]; then' "$D" || return 1
}
@test "NEGATIVE PROOF: the same messages with no notify off row are sent" {
  rm -f "$T/door/messages-not-kept"
  run send "messages-not-kept: no-reading:x"
  grep -q 'nudge-send system silas messages-not-kept' "$T/shim.log" || return 1
}
