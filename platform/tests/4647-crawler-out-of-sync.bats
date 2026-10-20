#!/usr/bin/env bats
# @test-type: unit — the alert's check reads a fixture of sync readings through READINGS_URL; nothing live
# @domain: heralds
# #4647 — the crawler-out-of-sync alert reads only the sync sensor's readings.
# A red case, a control, a "too soon" case, and the silent-sensor cases that
# must never read ok.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$(mktemp -d)"
  TODAY=$(TZ=America/New_York date +%F)
}

teardown() { rm -rf "$T"; }

check() {
  python3 - "$ROOT/proving/domains/alerts/crawler-out-of-sync.yml" > "$T/check.sh" <<'PY'
import sys, re
y = open(sys.argv[1]).read()
m = re.search(r'^check: \|\n((?:  .*\n|\n)+)', y, re.M)
print("\n".join(l[2:] for l in m.group(1).splitlines()))
PY
  READINGS_URL="file://$T/r.json" bash "$T/check.sh"
}

sync() { # sync <minutes ago> <subject metric value>... → door-shaped sync readings for today
  python3 - "$T/r.json" "$TODAY" "$@" <<'PY'
import json, sys, datetime
out, day, ago, a = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4:]
stamp = (datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(minutes=ago)).strftime("%Y%m%dt%H%M%Sz")
rows = [{"readingOfSensor": "sensor-sync", "readingSubject": a[i], "readingMetric": a[i+1], "readingValue": a[i+2], "readingDay": day, "readingRun": f"sensor-run-sync-{stamp}"} for i in range(0, len(a), 3)]
json.dump({"data": rows}, open(out, "w"))
PY
}

@test "a class still out of sync an hour after its run fires, naming the class and the count" {
  sync 90 codefile missing 4 codefile stale 0 endpoint missing 0 endpoint stale 0
  run check
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == out-of-sync:*codefile-missing=4* ]] || { echo "$output"; return 1; }
}

@test "control: every class at zero is ok" {
  sync 90 codefile missing 0 codefile stale 0 endpoint missing 0 endpoint stale 0
  run check
  [ "$output" = "ok" ] || { echo "$output"; return 1; }
}

@test "a gap from a run under an hour ago waits for the land crawl to catch up" {
  sync 20 codefile missing 4 codefile stale 0
  run check
  [ "$output" = "ok" ] || { echo "$output"; return 1; }
}

@test "negative proof: no sync reading at all is a finding, never ok" {
  echo '{"data":[{"readingOfSensor":"sensor-events","readingMetric":"events","readingValue":"5","readingRun":"sensor-run-events-20261019t030000z"}]}' > "$T/r.json"
  run check
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == "no-reading:sync" ]] || { echo "$output"; return 1; }
}

@test "negative proof: a sensor quiet for over 26 hours is stale even at zero" {
  sync 1700 codefile missing 0 codefile stale 0
  run check
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == stale-reading:* ]] || { echo "$output"; return 1; }
}
