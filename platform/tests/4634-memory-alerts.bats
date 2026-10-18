#!/usr/bin/env bats
# @test-type: unit — each alert's check reads a fixture of sensor readings through READINGS_URL; nothing live
# @domain: heralds
# #4634 — the three alerts on messages and events read only SensorReadings. Each
# has a red case, a control, and a "no reading" case that must never read ok.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$(mktemp -d)"
  TODAY=$(TZ=America/New_York date +%F)
  YDAY=$(TZ=America/New_York date -v-1d +%F)
}

teardown() { rm -rf "$T"; }

# check <alert> → runs the yml's check block against $T/r.json
check() {
  python3 - "$ROOT/proving/domains/alerts/$1.yml" > "$T/check.sh" <<'PY'
import sys, re
y = open(sys.argv[1]).read()
m = re.search(r'^check: \|\n((?:  .*\n|\n)+)', y, re.M)
print("\n".join(l[2:] for l in m.group(1).splitlines()))
PY
  READINGS_URL="file://$T/r.json" bash "$T/check.sh"
}

readings() { # readings <sensor metric day value>...
  python3 - "$T/r.json" "$@" <<'PY'
import json, sys
out, a = sys.argv[1], sys.argv[2:]
rows = [{"readingOfSensor": a[i], "readingMetric": a[i+1], "readingDay": a[i+2], "readingValue": a[i+3]} for i in range(0, len(a), 4)]
json.dump({"data": rows}, open(out, "w"))
PY
}

@test "backup-stale: a 26-hour copy fires, by sensor and hours" {
  readings messages backup-age "$TODAY" 2 events backup-age "$TODAY" 26
  run check memory-backup-stale
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"stale:events-copy-26h>24h"* ]] || return 1
}

@test "backup-stale control: fresh copies are ok" {
  readings messages backup-age "$TODAY" 2 events backup-age "$TODAY" 3
  run check memory-backup-stale
  [ "$output" = "ok" ]
}

@test "negative proof: no backup reading is a finding, never ok" {
  readings events events "$YDAY" 5
  run check memory-backup-stale
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"no-reading"* ]] || return 1
}

@test "not-kept: sent and kept that differ fire, equal is ok" {
  readings messages messages-sent "$YDAY" 690 messages messages-kept "$YDAY" 600
  run check messages-not-kept
  [[ "$output" == "gap:sent-690-kept-600-on-$YDAY" ]] || return 1
  readings messages messages-sent "$YDAY" 690 messages messages-kept "$YDAY" 690
  run check messages-not-kept
  [ "$output" = "ok" ]
}

@test "spine: not written for 12 minutes fires; yesterday with 0 events fires; fresh and busy is ok" {
  readings spine spine-write-age "$TODAY" 0.2 events events "$YDAY" 9
  run check spine-not-growing
  [[ "$output" == "stalled:spine-last-written-0.20h-ago>0.17h" ]] || return 1
  readings spine spine-write-age "$TODAY" 0.01 events events "$YDAY" 0
  run check spine-not-growing
  [[ "$output" == "gap:0-events-on-$YDAY" ]] || return 1
  readings spine spine-write-age "$TODAY" 0.01 events events "$YDAY" 9
  run check spine-not-growing
  [ "$output" = "ok" ]
}

@test "an unreadable readings source is a finding, never ok" {
  run env READINGS_URL="file://$T/missing.json" bash -c "true"
  python3 - "$ROOT/proving/domains/alerts/spine-not-growing.yml" > "$T/c.sh" <<'PY'
import sys, re
y = open(sys.argv[1]).read()
m = re.search(r'^check: \|\n((?:  .*\n|\n)+)', y, re.M)
print("\n".join(l[2:] for l in m.group(1).splitlines()))
PY
  run env READINGS_URL="file://$T/missing.json" bash "$T/c.sh"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == no-reading:* ]] || return 1
}
