#!/usr/bin/env bats
# @test-type: integration — runs the real promtail binary on the real config
# @domain: logs
# @card: 4454 · owner: kade
# Covers: platform/tests/4454-spine-json-in-loki.bats
# 4454-spine-json-in-loki.bats — what Jeff sees: a red names its cause from one
# Loki query. Every spine event reaches Loki as its whole JSON line, so
# `| json` finds its event, card and trace. Until 2026-10-09 the spine job ended
# with `output: source: message`: any event carrying a message (every *.failed
# one) became the bare sentence — 10,122 nightly case events in Loki, 0 found
# by `| json`.
#
# The pipeline is run through `promtail --stdin --dry-run` on the live config's
# own platform-chorus-log job. Config and binary live outside this repo
# (shared-observability, ~/bin), so their absence reports UNMEASURED, not red.

PROMTAIL_CONFIG="${PROMTAIL_CONFIG:-${HOME}/CascadeProjects/shared-observability/config/promtail/promtail-native.yaml}"
PROMTAIL_BIN="${PROMTAIL_BIN:-${HOME}/bin/promtail}"
EVENT='{"timestamp":"2026-10-09T09:00:00.000-0400","level":"error","appName":"chorus-events","component":"lifecycle","event":"test.case.failed","role":"nightly","card_id":4454,"trace":"tr-4454-neg","file":"a.bats","case":"x","message":"x failed in a.bats: boom"}'

setup() {
  [ -f "$PROMTAIL_CONFIG" ] || skip "UNMEASURED — promtail config not on this box ($PROMTAIL_CONFIG)"
  [ -x "$PROMTAIL_BIN" ] || skip "UNMEASURED — promtail binary not on this box ($PROMTAIL_BIN)"
  T="$(mktemp -d)"
}
teardown() { [ -n "${T:-}" ] && rm -rf "$T"; }

# A promtail config holding only <source config>'s spine job, pushing nowhere.
spine_job_config() {
  ruby -ryaml -e '
    job = YAML.load_file(ARGV[0])["scrape_configs"].find { |j| j["job_name"] == "platform-chorus-log" }
    abort "no platform-chorus-log job in #{ARGV[0]}" unless job
    puts YAML.dump("server" => { "disable" => true },
                   "positions" => { "filename" => "#{ARGV[1]}/pos.yaml" },
                   "clients" => [{ "url" => "http://127.0.0.1:9/loki/api/v1/push" }],
                   "scrape_configs" => [job])' "$1" "$T"
}

# The line promtail would send Loki for $EVENT. Dry-run exits before it flushes
# about half the time, so stdin is held open a moment and the run is retried;
# no line after five tries fails, it never passes empty.
loki_line() {
  local cfg="$1" out i
  for i in 1 2 3 4 5; do
    out=$({ printf '%s\n' "$EVENT"; "$PROMTAIL_BIN" --version >/dev/null 2>&1; "$PROMTAIL_BIN" --version >/dev/null 2>&1; } \
      | "$PROMTAIL_BIN" --stdin --dry-run --config.file="$cfg" 2>&1 | grep 'boom' | cut -f3)
    [ -n "$out" ] && { printf '%s\n' "$out"; return 0; }
  done
  echo "promtail printed no line for the event in 5 tries"
  return 1
}

# Event, card and trace as `| json` would read them off the line, or a failure.
fields_of() {
  printf '%s' "$1" | python3 -c 'import sys,json
d=json.loads(sys.stdin.read()); print(d["event"], d["card_id"], d["trace"])'
}

@test "a failed spine event reaches Loki whole: | json finds its event, card and trace" {
  spine_job_config "$PROMTAIL_CONFIG" > "$T/live.yaml"
  run loki_line "$T/live.yaml"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  run fields_of "$output"
  [ "$status" -eq 0 ] || { echo "line is not the JSON event: $output"; return 1; }
  [ "$output" = "test.case.failed 4454 tr-4454-neg" ]
}

@test "NEGATIVE PROOF — the old output: source: message stage strips the event to its sentence" {
  spine_job_config "$PROMTAIL_CONFIG" > "$T/live.yaml"
  ruby -ryaml -e '
    c = YAML.load_file(ARGV[0])
    c["scrape_configs"][0]["pipeline_stages"] << { "output" => { "source" => "message" } }
    puts YAML.dump(c)' "$T/live.yaml" > "$T/old.yaml"
  run loki_line "$T/old.yaml"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$output" = "x failed in a.bats: boom" ]
  run fields_of "$output"
  [ "$status" -ne 0 ]
}

@test "the guard fails loudly when the spine job is gone, never passes vacuously" {
  printf 'scrape_configs:\n  - job_name: something-else\n' > "$T/none.yaml"
  run spine_job_config "$T/none.yaml"
  [ "$status" -ne 0 ]
  [[ "$output" == *"no platform-chorus-log job"* ]] || return 1
}
