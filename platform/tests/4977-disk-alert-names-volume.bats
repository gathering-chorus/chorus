#!/usr/bin/env bats
# @test-type: unit — runs the jest suite that reads the disk alert's received text; no Grafana
# @domain: alerts
# @card: #4977 · owner: silas
# Checks platform/scripts/monitor-alert, designing/domains/monitors/bedroom-disk-full-monitor-4970.json
# and platform/api/src/handlers/grafana-alerts.ts.
#
# #4977 AC1 — the disk alert's text names the volume over the line. The proof is
# the text alertText makes from the labels Grafana sent on 2026-11-14 20:32, not
# the query. A text with no volume name fails (the jest suite's negative proof).

@test "the disk alert text names its volume, three volumes read as three texts, and a text with no volume is caught" {
  cd "$BATS_TEST_DIRNAME/../api"
  run npx jest tests/handlers/disk-alert-text-4977.test.ts
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"3 passed"* ]] || return 1
}
@test "the monitor row says which volume, in Grafana's own template words" {
  M="$BATS_TEST_DIRNAME/../../designing/domains/monitors/bedroom-disk-full-monitor-4970.json"
  python3 -c "import json,sys;s=json.load(open(sys.argv[1]))['data']['monitorSays'];assert '{{ \$labels.mountpoint }}' in s, s" "$M" || return 1
}
