#!/usr/bin/env bats
# @test-type: unit — reads two repo files; no live dagu, no service
# @domain: pipelines — the nightly pipeline's one-at-a-time rule (#4540)
#
# #4540 — the nightly runs one at a time. werk-test's own lock (and its 4008
# suite) is retired; dagu holds the single flight now: the nightly dag sits on
# the `nightly` queue and dagu.yaml caps that queue at 1. Measured 2026-10-16 on
# a scratch dagu 2.18.2: a second enqueue sat Queued until the first ended,
# while `max_active_runs: 1` did NOT stop a second `dagu start` — so the queue
# is the rule this suite guards, and nothing else counts.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CONFIG="$REPO/platform/config/dagu.yaml"
  NIGHTLY="$REPO/platform/pipelines/nightly.yaml"
}

# single_flight <dagu.yaml> <nightly.yaml>: prints why it is not single-flight
# and exits 1, or exits 0 when the nightly dag is on a queue capped at 1.
single_flight() {
  ruby -ryaml -e '
    cfg = YAML.load_file(ARGV[0]); dag = YAML.load_file(ARGV[1])
    q = dag["queue"]
    abort "nightly dag names no queue" if q.nil? || q.to_s.empty?
    queues = ((cfg["queues"] || {})["config"] || [])
    abort "dagu.yaml has queues disabled" unless (cfg["queues"] || {})["enabled"]
    row = queues.find { |r| r["name"] == q }
    abort "dagu.yaml has no queue named #{q}" if row.nil?
    abort "queue #{q} allows #{row["max_concurrency"]} at once, not 1" unless row["max_concurrency"] == 1
  ' "$1" "$2"
}

@test "the nightly dag is on a dagu queue that runs one at a time" {
  [ -f "$CONFIG" ] || { echo "missing $CONFIG"; return 1; }
  [ -f "$NIGHTLY" ] || { echo "missing $NIGHTLY — a guard whose target is gone must fail"; return 1; }
  run single_flight "$CONFIG" "$NIGHTLY"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "negative proof: a queue that allows two at once is refused" {
  tmp="$BATS_TEST_TMPDIR"
  ruby -ryaml -e '
    c = YAML.load($stdin.read); c["queues"]["config"].each { |r| r["max_concurrency"] = 2 if r["name"] == "nightly" }
    puts c.to_yaml' < "$CONFIG" > "$tmp/dagu.yaml"
  run single_flight "$tmp/dagu.yaml" "$NIGHTLY"
  [ "$status" -ne 0 ]
  [[ "$output" == *"allows 2 at once"* ]] || return 1
}

@test "negative proof: a nightly dag with no queue is refused" {
  tmp="$BATS_TEST_TMPDIR"
  ruby -ryaml -e 'd = YAML.load_file(ARGV[0]); d.delete("queue"); puts d.to_yaml' "$NIGHTLY" > "$tmp/nightly.yaml"
  run single_flight "$CONFIG" "$tmp/nightly.yaml"
  [ "$status" -ne 0 ]
  [[ "$output" == *"names no queue"* ]] || return 1
}
