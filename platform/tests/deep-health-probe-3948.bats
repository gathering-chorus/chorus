#!/usr/bin/env bats
# @test-type: unit — signal from fixture servers on ephemeral localhost ports;
# no live service consulted.
#
# #3948 — the endpoint probe stops lying in both directions. Six false
# "unreachable — team chat broken" DEGRADED banners came from one 5s curl
# being the whole verdict. These tests drive the probe loop's logic against
# controlled fixtures and prove: dead → DOWN (still fails, the guard keeps
# teeth), slow-but-alive → UNMEASURABLE (warns, never degrades), healthy →
# clean. (#3734: each rule shown to fail.)
load test_helper

PROBE_SNIPPET() {  # extracted probe semantics, driven per-URL
  url="$1"
  reachable=0; probe_code="000"; probe_ms="?"; probe_exit=0
  for attempt in 1 2; do
    probe_code=$(curl -s --max-time 2 -o /dev/null -w "%{http_code}" "$url" 2>/dev/null); probe_exit=$?
    if [ "$probe_exit" -eq 0 ] && [ "$probe_code" -ge 200 ] && [ "$probe_code" -lt 400 ]; then
      reachable=1; break
    fi
    [ "$attempt" -eq 1 ] && sleep 1
  done
  echo "$reachable $probe_exit $probe_code"
}

@test "healthy endpoint → reachable on first pass" {
  python3 -m http.server 39481 --bind 127.0.0.1 >/dev/null 2>&1 &
  SRV=$!; sleep 1
  run PROBE_SNIPPET "http://127.0.0.1:39481/"
  kill $SRV 2>/dev/null
  [[ "$output" == 1\ * ]]
}

@test "NEGATIVE: dead port → DOWN (exit 7), never unmeasurable" {
  run PROBE_SNIPPET "http://127.0.0.1:39482/"
  read -r reach exitc code <<< "$output"
  [ "$reach" -eq 0 ]
  [ "$exitc" -eq 7 ]   # connection refused = DOWN — degrades, correctly
}

@test "NEGATIVE: slow-but-alive → timeout exit 28 = UNMEASURABLE, not down" {
  # a listener that accepts but never answers within the probe ceiling
  python3 -c "
import socket,time
s=socket.socket(); s.bind(('127.0.0.1',39483)); s.listen(1)
while True:
    c,_=s.accept(); time.sleep(10); c.close()" >/dev/null 2>&1 &
  SRV=$!; sleep 1
  run PROBE_SNIPPET "http://127.0.0.1:39483/"
  kill $SRV 2>/dev/null
  read -r reach exitc code <<< "$output"
  [ "$reach" -eq 0 ]
  [ "$exitc" -eq 28 ]  # timeout = UNMEASURABLE — warns, never degrades
}

@test "the deployed script carries the split (no bare 'unreachable' verdicts)" {
  grep -q "UNMEASURABLE" "${CHORUS_ROOT}/platform/scripts/deep-health.sh"
  grep -q 'DOWN — code=' "${CHORUS_ROOT}/platform/scripts/deep-health.sh"
  ! grep -qE '^\s*FAILURES\+=\("\$name: unreachable' "${CHORUS_ROOT}/platform/scripts/deep-health.sh"
}
