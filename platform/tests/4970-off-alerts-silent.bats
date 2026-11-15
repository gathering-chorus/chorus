#!/usr/bin/env bats
# @test-type: unit — drives the real ops-nudge with a stub shim and a file:// door; no pulse, no store
# @domain: alerts
# @card: #4970 · owner: silas
# Checks platform/scripts/ops-nudge and the AC6 list on #4970.
#
# #4970 AC6 — after the land every OFF alert notifies no one. One field does it for
# both kinds: an Alert row with notify off. Grafana alerts honour it in the webhook
# (AC5, jest); script alerts honour it here, at ops-nudge, the one way an ops
# script sends. A notify-off firing is recorded (alert.quiet) and nothing is sent;
# a row without it, or a door that cannot be read, still sends (fail open).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
  # stub shim: records what it was asked to do
  printf '#!/bin/sh\necho "$@" >> "%s/shim.log"\n' "$T" > "$T/shim"; chmod +x "$T/shim"
  mkdir -p "$T/door"
  printf '{"data":{"notify":"off"}}' > "$T/door/lancedb-stale"
  printf '{"data":{"alertSource":"script"}}' > "$T/door/loki-refusing"
  printf '{"data":{"notify":"off"}}' > "$T/door/sensor-run-not-green"
  unset CHORUS_ROLE
}

send() { env CHORUS_HOOK_SHIM="$T/shim" ALERTS_DOOR="${DOOR:-file://$T/door}" OPS_ALERT="${RULE:-}" bash "$ROOT/platform/scripts/ops-nudge" silas "$1"; }

@test "an alert whose row says notify off sends nothing and is recorded as alert.quiet" {
  run send "lancedb-stale: dir=/x age=5201h"
  [ "$status" -eq 0 ]
  ! grep -q 'nudge-send' "$T/shim.log" || return 1
  grep -q 'chorus-log alert.quiet system alertname=lancedb-stale to=silas' "$T/shim.log" || return 1
}
@test "NEGATIVE PROOF: an alert whose row has no notify still sends" {
  run send "loki-refusing: refused=11 of 460"
  [ "$status" -eq 0 ]
  grep -q 'nudge-send system silas loki-refusing: refused=11 of 460' "$T/shim.log" || return 1
}
@test "NEGATIVE PROOF: a door that cannot be read silences nothing (fails open)" {
  DOOR="http://127.0.0.1:9/owl/v1/alerts/alerts" run send "lancedb-stale: dir=/x"
  grep -q 'nudge-send system silas lancedb-stale: dir=/x' "$T/shim.log" || return 1
}
@test "a message with no row-name prefix is sent as before" {
  run send "free text with no prefix"
  grep -q 'nudge-send system silas free text with no prefix' "$T/shim.log" || return 1
}
@test "a Grafana alert is gated by the rule uid the webhook hands over, not its text" {
  RULE=sensor-run-not-green run send "grafana: Sensor run not green — sensor messages, value 1"
  ! grep -q 'nudge-send' "$T/shim.log" || return 1
  grep -q 'alert.quiet system alertname=sensor-run-not-green' "$T/shim.log" || return 1
}
@test "NEGATIVE PROOF: a Grafana alert whose rule row has no notify still sends" {
  RULE=loki-refusing run send "grafana: something — value 1"
  grep -q 'nudge-send system silas grafana: something' "$T/shim.log" || return 1
}
@test "every OFF alert on Jeff's 18:12 list is set quiet by this land" {
  L="$ROOT/platform/scripts/land-4970-rows"
  for a in buzz-shadow-not-matching crawler-out-of-sync lancedb-stale messages-not-kept; do
    grep -q "\"$a\"" "$L" || return 1
  done
  for a in sensor-run-not-green shacl-violations-above-baseline run-finished-pass; do
    grep -q "(\"$a\", {" "$L" || return 1
  done
  [ "$(grep -c '"notify": "off"' "$L")" -ge 4 ] || return 1
}
@test "a pipeline pass wakes under run-finished-pass (DROP); a failure carries no row and always wakes" {
  R="$ROOT/platform/scripts/run-finished"
  grep -q 'OPS_ALERT="run-finished-pass"' "$R" || return 1
  grep -q 'OPS_ALERT="\$OPS_ALERT" bash "\$S/ops-nudge"' "$R" || return 1
  # the pass row name is set only inside succeeded)
  awk '/^  failed\)/,/;;$/' "$R" | grep -q 'run-finished-pass' && return 1
  return 0
}
@test "NEGATIVE PROOF: an OPS_ALERT inherited from the caller cannot silence a failure wake" {
  R="$ROOT/platform/scripts/run-finished"
  init=$(grep -n '^OPS_ALERT=""$' "$R" | cut -d: -f1)
  case_at=$(grep -n '^case "\$STATUS" in' "$R" | cut -d: -f1)
  [ -n "$init" ] && [ -n "$case_at" ] || return 1
  [ "$init" -lt "$case_at" ] || return 1
}
