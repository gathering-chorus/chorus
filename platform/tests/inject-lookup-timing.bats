#!/usr/bin/env bats
# @domain: hooks
# @test-type: unit
# #4815 — context.inject.response names each lookup's own time, so a slow prompt
# says which lookup made it slow. Negative proof (#3734): a fixture lookup that
# sleeps 3 s is named the slowest.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/platform/services/chorus-hooks/target/$(id -un)}"
}

@test "AC1+AC3: each lookup's ms is logged by name, and a 3 s fixture lookup is named slowest" {
  run cargo test --manifest-path "$ROOT/platform/services/chorus-hooks/Cargo.toml" --bin chorus-hooks -q -- a_three_second_lookup_is_named_slowest no_lookups_names_no_slowest
  [ "$status" -eq 0 ]
  [[ "$output" == *"2 passed"* ]] || return 1
}

# #4835 — the unit proofs behind the speed fix: forty prompts share one Loki read,
# the memory files are read once per minute, and the Boston offset is asked once
# per minute instead of a `date` fork per log line. Negative proofs: with sharing
# off every caller reads, an empty Loki answer is never kept, the OS path forks.
@test "#4835: shared Loki read, cached memory corpus, cached Boston offset (with negative proofs)" {
  run cargo test --manifest-path "$ROOT/platform/services/chorus-hooks/Cargo.toml" --bin chorus-hooks -q -- \
    concurrent_callers_share_one_read ttl_zero_reads_every_time memory_match_needs_two_keywords offset_cache_4835
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"5 passed"* ]] || return 1
}

# #4835 AC1 — 40 prompts at once through the running daemon, p95 under 2 s.
# Needs the daemon and a quiet box: at or above the core count the number measures
# the box, so it reports UNMEASURED (skip), never a pass (#4071's rule).
@test "#4835 AC1: 40 concurrent prompts, p95 under 2s on a quiet box" {
  sock="${CHORUS_HOOKS_SOCK:-$HOME/.chorus/run/chorus-hooks.sock}"
  # a role account reaches the daemon through its agent socket dir
  [ -S "$sock" ] || sock="${CHORUS_HOOKS_RUN_DIR:-$HOME/.chorus/run}/chorus-hooks.sock"
  [ -S "$sock" ] || skip "UNMEASURED: no hooks daemon socket on this box"
  load=$(sysctl -n vm.loadavg | awk '{print int($2)}'); ncpu=$(sysctl -n hw.ncpu)
  [ "$load" -lt "$ncpu" ] || skip "UNMEASURED: load $load >= $ncpu cores, the number would measure the box"
  out="$BATS_TEST_TMPDIR/times"
  for i in $(seq 1 40); do
    ( p=$(printf '{"hook_event_name":"UserPromptSubmit","prompt":"4835 load %s","session_id":"bats-4835-%s","cwd":"%s"}' "$i" "$i" "$ROOT")
      curl -s -o /dev/null -m 15 -w '%{time_total}\n' --unix-socket "$sock" -H 'Content-Type: application/json' --data "$p" http://localhost/user-prompt-submit ) &
  done > "$out"
  wait
  n=$(wc -l < "$out" | tr -d ' ')
  [ "$n" -eq 40 ] || return 1
  p95=$(sort -n "$out" | sed -n '38p')
  echo "p95=${p95}s"
  awk -v p="$p95" 'BEGIN { exit !(p < 2.0) }' || return 1
}
