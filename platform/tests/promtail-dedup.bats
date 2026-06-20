#!/usr/bin/env bats
# @test-type: integration — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
load test_helper
# promtail-dedup.bats — verify no duplicate Promtail streams in Loki (#1986)
# What Jeff sees: Loki queries return one stream per log file, not duplicates.
# Prior work: #1984 added glob-based scrape for Chorus/Gathering log dirs.
# Log evidence: chorus-api.log had 2 Loki streams (job=daemon-logs + job=chorus-api).
# Fix: __path_exclude__ in promtail-native.yaml excludes dedicated files from glob.

PROMTAIL_CONFIG="${HOME}/CascadeProjects/shared-observability/config/promtail/promtail-native.yaml"
LOKI="http://localhost:3102"

@test "Promtail config is valid" {
  ${HOME}/bin/promtail -config.file="$PROMTAIL_CONFIG" -check-syntax 2>&1 | grep -q "Valid config"
}

@test "chorus-daemon-logs has __path_exclude__ for dedicated files" {
  # The exclude line should contain chorus-api.log and chorus.log
  grep "__path_exclude__.*chorus-api" "$PROMTAIL_CONFIG"
}

@test "gathering-daemon-logs has __path_exclude__ for dedicated files" {
  grep "__path_exclude__.*cloudflared" "$PROMTAIL_CONFIG"
}

@test "no recent daemon-logs entries for chorus-api.log (last 1 min)" {
  count=$(curl -s "${LOKI}/loki/api/v1/query" \
    --data-urlencode "query=count_over_time({job=\"daemon-logs\",filename=\"${HOME}/Library/Logs/Chorus/chorus-api.log\"}[1m])" \
    2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); vals=d.get('data',{}).get('result',[]); print(sum(int(v['value'][1]) for v in vals))" 2>/dev/null)
  [ "$count" -eq 0 ]
}
