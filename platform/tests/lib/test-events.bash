# @test-type: unit — helper for bats suites and their fixtures, not a suite
# #4454 — a test or fixture says what it is doing, on the run's trace.
#   test_event test.fixture.ready fixture=stub-upstream port=51625 ready_ms=120
# Writes one `chorus-log --batch` line to $CHORUS_TEST_EVENTS; werk-test
# forwards the file's test.* lines to the spine after each unit (see
# test-events.cjs). Unset outside werk-test: then it does nothing.
test_event() {
  [ -n "${CHORUS_TEST_EVENTS:-}" ] || return 0
  local event="$1"; shift
  local line="$event"$'\t'"tests" kv
  for kv in "$@" "at=$(date -u +%Y-%m-%dT%H:%M:%SZ)" ${CHORUS_TRACE_ID:+"trace=$CHORUS_TRACE_ID"} ${CHORUS_CARD_ID:+"card=$CHORUS_CARD_ID"}; do
    line+=$'\t'"${kv//[$'\t\r\n']/ }"
  done
  printf '%s\n' "$line" >> "$CHORUS_TEST_EVENTS" 2>/dev/null || true
}

_te_ms() { perl -MTime::HiRes=time -e 'printf "%d", time*1000'; }

# stub_ready <name> <port> [pid] [tries=50] — wait until a stub answers on
# 127.0.0.1:<port> (any HTTP reply), logging test.fixture.started, then
# test.fixture.ready with how long it took, or test.fixture.failed with why.
# Returns 0 when it answers, 1 when it never did.
stub_ready() {
  local name="$1" port="$2" pid="${3:-}" tries="${4:-50}" t0 i
  t0=$(_te_ms)
  test_event test.fixture.started fixture="$name" port="$port" ${pid:+pid=$pid} "message=$name starting on port $port"
  for ((i = 0; i < tries; i++)); do
    if curl -s -o /dev/null --max-time 1 "http://127.0.0.1:$port/" 2>/dev/null; then
      test_event test.fixture.ready fixture="$name" port="$port" ready_ms=$(($(_te_ms) - t0)) "message=$name ready on port $port"
      return 0
    fi
    if [ -n "$pid" ] && ! kill -0 "$pid" 2>/dev/null; then
      test_event test.fixture.failed fixture="$name" port="$port" elapsed_ms=$(($(_te_ms) - t0)) level=error \
        "reason=exited before answering" "message=$name on port $port exited before answering"
      return 1
    fi
    sleep 0.1
  done
  test_event test.fixture.failed fixture="$name" port="$port" elapsed_ms=$(($(_te_ms) - t0)) level=error \
    "reason=no answer after $tries tries" "message=$name on port $port never answered"
  return 1
}

# fixture_ready <name> <port> <pid> <tries> <sleep> <probe...> — stub_ready for
# a stub whose readiness is its own request (a query it counts, a probe path):
# runs <probe...> until it succeeds, with the same events.
fixture_ready() {
  local name="$1" port="$2" pid="$3" tries="$4" pause="$5" t0 i; shift 5
  t0=$(_te_ms)
  test_event test.fixture.started fixture="$name" port="$port" ${pid:+pid=$pid} "message=$name starting on port $port"
  for ((i = 0; i < tries; i++)); do
    if "$@" >/dev/null 2>&1; then
      test_event test.fixture.ready fixture="$name" port="$port" ready_ms=$(($(_te_ms) - t0)) "message=$name ready on port $port"
      return 0
    fi
    sleep "$pause"
  done
  test_event test.fixture.failed fixture="$name" port="$port" elapsed_ms=$(($(_te_ms) - t0)) level=error \
    "reason=no answer after $tries tries" "message=$name on port $port never answered"
  return 1
}
