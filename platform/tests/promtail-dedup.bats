#!/usr/bin/env bats
# @test-type: integration — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
# @domain: logs — the product domain this suite guards (#4334)
load test_helper
# promtail-dedup.bats — verify no duplicate Promtail streams in Loki (#1986)
# What Jeff sees: Loki queries return one stream per log file, not duplicates.
# Prior work: #1984 added glob-based scrape for Chorus/Gathering log dirs.
# Log evidence: chorus-api.log had 2 Loki streams (job=daemon-logs + job=chorus-api).
# Fix: __path_exclude__ in promtail-native.yaml excludes dedicated files from glob.
#
# #4336: the exclude checks were greps of the config text. They now ask the parsed
# config which jobs would scrape a given file (fixtures/4336/promtail-routes.rb applies
# each target's __path__ and __path_exclude__ globs), so an exclude that sits in the
# wrong job, on the wrong target, or misses the file goes red. A real promtail
# scrape of a fixture log was tried and cannot run hermetically: promtail never exits
# on its own and ignores SIGALRM, so it would need to be killed. The live-stream
# check is gated to RUN_LIVE_INTEGRATION. The config and binary live outside this
# repo (shared-observability, ~/bin), so their absence reports UNMEASURED, not red.

PROMTAIL_CONFIG="${HOME}/CascadeProjects/shared-observability/config/promtail/promtail-native.yaml"
PROMTAIL_BIN="${HOME}/bin/promtail"
LOKI="http://localhost:3102"
ROUTES="${BATS_TEST_DIRNAME}/fixtures/4336/promtail-routes.rb"
LOGS="${HOME}/Library/Logs"

need_config() {
  [ -f "$PROMTAIL_CONFIG" ] || skip "UNMEASURED — promtail config not on this box ($PROMTAIL_CONFIG) (#4336)"
}

# jobs that would scrape <file> under <config>, comma-joined
routes() { ruby "$ROUTES" "$1" "$2" | paste -sd, -; }

@test "Promtail config is valid" {
  need_config
  [ -x "$PROMTAIL_BIN" ] || skip "UNMEASURED — promtail binary not on this box ($PROMTAIL_BIN) (#4336)"
  run "$PROMTAIL_BIN" -config.file="$PROMTAIL_CONFIG" -check-syntax
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Valid config"
}

@test "chorus-api.log is scraped by its own job only, never also by the daemon-logs glob" {
  need_config
  # positive control: a file with no dedicated job IS picked up by the glob, so an
  # empty answer below cannot pass for "excluded"
  [ "$(routes "$PROMTAIL_CONFIG" "$LOGS/Chorus/some-other-daemon.log")" = "daemon-logs" ]
  [ "$(routes "$PROMTAIL_CONFIG" "$LOGS/Chorus/chorus-api.log")" = "chorus-api" ]
}

@test "cloudflared.log is scraped by its own job only, never also by the gathering daemon-logs glob" {
  need_config
  [ "$(routes "$PROMTAIL_CONFIG" "$LOGS/Gathering/some-other-daemon.log")" = "daemon-logs" ]
  [ "$(routes "$PROMTAIL_CONFIG" "$LOGS/Gathering/cloudflared.log")" = "cloudflared" ]
}

@test "NEGATIVE PROOF: a config whose glob job lost its exclude reports the duplicate" {
  need_config
  fixture="$BATS_TEST_TMPDIR/promtail.yaml"
  sed -E '/__path_exclude__:.*Logs\/(Chorus|Gathering)\/\{/d' "$PROMTAIL_CONFIG" > "$fixture"
  [ "$(routes "$fixture" "$LOGS/Chorus/chorus-api.log")" = "chorus-api,daemon-logs" ]
  [ "$(routes "$fixture" "$LOGS/Gathering/cloudflared.log")" = "cloudflared,daemon-logs" ]
}

@test "no recent daemon-logs entries for chorus-api.log (last 1 min)" {
  [ "${RUN_LIVE_INTEGRATION:-}" = "true" ] || skip "UNMEASURED — queries the live Loki on :3102; set RUN_LIVE_INTEGRATION=true (#4336)"
  count=$(curl -s "${LOKI}/loki/api/v1/query" \
    --data-urlencode "query=count_over_time({job=\"daemon-logs\",filename=\"${HOME}/Library/Logs/Chorus/chorus-api.log\"}[1m])" \
    2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin); vals=d.get('data',{}).get('result',[]); print(sum(int(v['value'][1]) for v in vals))" 2>/dev/null)
  [ -n "$count" ]
  [ "$count" -eq 0 ]
}
