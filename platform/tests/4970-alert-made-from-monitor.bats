#!/usr/bin/env bats
# @test-type: unit — builds the Alert row from monitor and sensor fixtures; no store, no network
# @domain: monitors
# @card: #4970 · owner: silas
# Checks platform/scripts/monitor-alert and platform/scripts/health-check-bedroom.sh.
#
# #4970 AC3 — the Grafana rule for disks is made from the monitor row, not written
# by hand: monitor-alert turns (monitor, sensor) into the Alert row convergence-land
# lands. The line lives in metricQuery because convergence-land's threshold is
# fixed at > 0 (Kade, 2026-11-14 17:37). The old script check is removed in the
# same land: health-check-bedroom.sh no longer judges disk fullness.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  MA="$ROOT/platform/scripts/monitor-alert"
  F="$ROOT/designing/domains/monitors"
  TMPD="$(mktemp -d)"
}
teardown() { rm -rf "$TMPD"; }

built() { "$MA" build "$F/bedroom-disk-full-monitor-4970.json" "$F/infra-disk-sensor-4970.json"; }
jget() { python3 -c "import json,sys;print(json.load(sys.stdin)[sys.argv[1]])" "$1"; }

@test "the Alert row's query is the sensor's query held to the monitor's line" {
  q="$(python3 -c "import json;print(json.load(open('$F/infra-disk-sensor-4970.json'))['data']['sensorQuery'])")"
  [ "$(built | jget metricQuery)" = "($q) > 99" ]
  [ "$(built | jget metricDatasource)" = "prometheus" ]
}
@test "the owner and the action come from the monitor" {
  [ "$(built | jget alertRoute)" = "silas" ]
  [ "$(built | jget ownedBy)" = "chorus:principal-silas" ]
  built | jget comment | grep -q "Action: Free space on the named volume"
}
@test "a changed line changes the rule with no code change" {
  sed 's/"monitorLine": "99"/"monitorLine": "97.5"/' "$F/bedroom-disk-full-monitor-4970.json" > "$TMPD/m.json"
  "$MA" build "$TMPD/m.json" "$F/infra-disk-sensor-4970.json" | jget metricQuery | grep -q ') > 97.5$'
}
@test "below flips the comparison" {
  sed 's/"monitorWhen": "above"/"monitorWhen": "below"/' "$F/bedroom-disk-full-monitor-4970.json" > "$TMPD/m.json"
  "$MA" build "$TMPD/m.json" "$F/infra-disk-sensor-4970.json" | jget metricQuery | grep -q ') < 99$'
}
@test "NEGATIVE PROOF: a monitor with no action makes no rule" {
  python3 -c "import json;d=json.load(open('$F/bedroom-disk-full-monitor-4970.json'));del d['data']['monitorAction'];json.dump(d,open('$TMPD/m.json','w'))"
  run "$MA" build "$TMPD/m.json" "$F/infra-disk-sensor-4970.json"
  [ "$status" -eq 3 ]; [[ "$output" == *"no monitorAction"* ]]
}
@test "NEGATIVE PROOF: a sensor that is not a Grafana pointer makes no rule" {
  python3 -c "import json;d=json.load(open('$F/infra-disk-sensor-4970.json'));del d['data']['sensorSource'];json.dump(d,open('$TMPD/s.json','w'))"
  run "$MA" build "$F/bedroom-disk-full-monitor-4970.json" "$TMPD/s.json"
  [ "$status" -eq 3 ]; [[ "$output" == *"not a Grafana pointer"* ]]
}
@test "NEGATIVE PROOF: a monitor pointed at another sensor makes no rule" {
  sed 's/chorus:infra-disk/chorus:other/' "$F/bedroom-disk-full-monitor-4970.json" > "$TMPD/m.json"
  run "$MA" build "$TMPD/m.json" "$F/infra-disk-sensor-4970.json"
  [ "$status" -eq 3 ]; [[ "$output" == *"reads other, not infra-disk"* ]]
}
@test "health-check-bedroom.sh no longer judges disk fullness" {
  S="$ROOT/platform/scripts/health-check-bedroom.sh"
  [ -f "$S" ]
  ! grep -qE '^[^#]*\bdf\b' "$S" || return 1
  ! grep -q 'bedroom-disk-last-pct' "$S" || return 1
  ! grep -q 'external disk at' "$S" || return 1
}
@test "NEGATIVE PROOF: a line that is not a plain number makes no rule (it would be pasted into PromQL)" {
  for bad in nan inf 1e3 " 99" "99 or 1"; do
    python3 -c "import json,sys;d=json.load(open('$F/bedroom-disk-full-monitor-4970.json'));d['data']['monitorLine']=sys.argv[1];json.dump(d,open('$TMPD/m.json','w'))" "$bad"
    run "$MA" build "$TMPD/m.json" "$F/infra-disk-sensor-4970.json"
    [ "$status" -eq 3 ] || return 1
  done
}
@test "the rule waits 5m before firing, so Grafana's own 'for' never reads as a hand edit" {
  [ "$(built | jget forDuration)" = "5m" ] || return 1
}
