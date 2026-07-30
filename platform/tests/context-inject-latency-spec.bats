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
  local uniq="latency-ceiling-$(date +%s)-$RANDOM"
  envelope_ms "$uniq prime" >/dev/null
  warm=$(envelope_ms "$uniq prime")
  echo "warm=${warm}ms" >&2
  [ "$warm" -lt 400 ]
}
