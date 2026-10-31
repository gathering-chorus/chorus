#!/usr/bin/env bats
# @test-type: integration:perf — times 40 prompts at once through the running daemon
# @domain: hooks
# @needs: machine — times the running chorus-hooks daemon (#4539: card runs leave it to the nightly)
# #4835 AC1. A card's own run would time the daemon from BEFORE its change, so this
# runs on main after the land, where the number is the change's.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
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
