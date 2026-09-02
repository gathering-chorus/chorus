#!/usr/bin/env bats
# @test-type: integration — measures live latency against the running daemon
# #3710 — retiered. This spec times real requests through the chorus-hooks unix
# socket at ~/.chorus/run/chorus-hooks.sock and compares warm-vs-cold latency.
# That needs the daemon RUNNING; it was declared a "static source/shape guard,
# hermetic", which it has never been — on any box without the socket it failed
# at [ -S "$SOCKET" ] rather than reporting that there was nothing to measure.
# Latency numbers are also meaningless on a shared CI runner. The local nightly
# runs it where the daemon exists and the timings mean something.
load test_helper
# context-inject-latency-spec.bats (#2231)
#
# Per-turn wall-clock cost of UserPromptSubmit → context-synthesis envelope.
# The inflection commit 49b5837c wired pulse + spine + athena into per-prompt
# synthesis; the correct fix, but it stacked three redundant operations onto
# every prompt cycle. This spec asserts:
#
#   1. Warm-cache latency is substantially lower than cold (caching engaged).
#   2. A single prompt cycle completes under a soft ceiling.
#
# Pre-#2231 cold latency is ~800ms; warm latency is the same (no cache).
# Post-#2231 warm latency should drop well below 400ms.

CHORUS_ROOT="${CHORUS_ROOT:-${CHORUS_ROOT}}"
SOCKET="$HOME/.chorus/run/chorus-hooks.sock"  # #3617: daemon serves from ~/.chorus/run since the 7/8 lockout fix

# #4071 — a latency RATIO measured on a saturated box measures the box, not the
# cache: red at load 60 (cold 2081ms, warm 1864ms), green at load 9, same
# daemon, same code. Below the core count the numbers mean something; at or
# above it this spec reports UNMEASURED (a bats skip — never pass, never fail).
# LOAD_1MIN / NCPU are the fixture seam so the gate itself has proofs.
load_gate_reason() {
  local load ncpu
  load="${LOAD_1MIN:-$(sysctl -n vm.loadavg 2>/dev/null | awk '{print $2}')}"
  ncpu="${NCPU:-$(sysctl -n hw.ncpu 2>/dev/null || echo 8)}"
  [ -n "$load" ] || { echo "UNMEASURED — load average unreadable"; return 0; }
  python3 -c "import sys; l=float('$load'); n=float('$ncpu'); sys.exit(0 if l >= n else 1)" \
    && echo "UNMEASURED — 1-minute load ${load} at or above ${ncpu} cores; latency would measure the box, not the cache"
  return 0
}
require_quiet_box() {
  local why; why=$(load_gate_reason)
  [ -z "$why" ] || skip "$why"
}

envelope_ms() {
  local prompt="$1" session="${2:-latency-spec}"
  local payload
  payload=$(printf '{"hook_event_name":"UserPromptSubmit","prompt":"%s","session_id":"%s"}' "$prompt" "$session")
  python3 -c "
import json, subprocess, time, sys
payload = sys.stdin.read()
t0 = time.time()
subprocess.run(
    ['curl', '-s', '--unix-socket', '$SOCKET',
     '-X', 'POST', '-H', 'Content-Type: application/json',
     '--data', payload, 'http://localhost/user-prompt-submit'],
    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
)
print(int((time.time() - t0) * 1000))
" <<<"$payload"
}

@test "prereq: chorus-hooks socket exists" {
  [ -S "$SOCKET" ]
}

@test "warm-cache latency is lower than cold-cache latency" {
  require_quiet_box
  # Use a unique prompt so cold call actually misses the cache.
  local uniq="latency-spec-$(date +%s)-$RANDOM"
  cold=$(envelope_ms "$uniq first call warms cache")
  warm=$(envelope_ms "$uniq first call warms cache")
  echo "cold=${cold}ms warm=${warm}ms" >&2
  # Warm should be meaningfully faster. Require >=30% drop.
  [ "$cold" -gt 0 ]
  [ "$warm" -lt "$cold" ] || { echo "warm (${warm}ms) not faster than cold (${cold}ms) — caching not engaged" >&2; false; }
  # Soft ratio check
  python3 -c "
cold, warm = $cold, $warm
assert warm * 100 / cold <= 70, f'warm ratio {warm*100/cold:.0f}% of cold — expected <=70%'
"
}

@test "warm-cache latency under 400ms ceiling" {
  require_quiet_box
  local uniq="latency-ceiling-$(date +%s)-$RANDOM"
  envelope_ms "$uniq prime" >/dev/null
  warm=$(envelope_ms "$uniq prime")
  echo "warm=${warm}ms" >&2
  [ "$warm" -lt 400 ]
}

# #4071 proofs for the gate itself (#3734): it must open on a quiet box and
# close on a busy one — with the seam, no live load needed.
@test "load gate: a quiet box measures (positive control)" {
  out=$(LOAD_1MIN=3.2 NCPU=8 load_gate_reason)
  [ -z "$out" ] || { echo "gate closed on a quiet box: $out" >&2; return 1; }
}

@test "NEGATIVE PROOF: load gate closes at and above the core count" {
  out=$(LOAD_1MIN=8.0 NCPU=8 load_gate_reason)
  [[ "$out" == UNMEASURED* ]] || { echo "gate open at load == cores: '$out'" >&2; return 1; }
  out=$(LOAD_1MIN=57.7 NCPU=8 load_gate_reason)
  [[ "$out" == UNMEASURED* ]] || { echo "gate open at load 57.7: '$out'" >&2; return 1; }
  out=$(LOAD_1MIN= NCPU=8 load_gate_reason)
  [[ "$out" == UNMEASURED* ]] || { echo "unreadable load must not read as quiet: '$out'" >&2; return 1; }
}
