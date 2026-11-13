#!/usr/bin/env bats
# @test-type: contract — the page and the route's handler; no server, no store.
# @domain: policies
# @card: #4964
# @owner: silas
#
# #4964 AC4 — the demo shows one table of the 15 with today's count each, worst first, on a
# page Jeff can open from his phone (not localhost): loom/policy-counts.html?gate=quality,data,product,code,
# served by chorus-api from disk, reading /api/chorus/policies/counts same-origin. Several gates
# make one table, each row naming its gate (policy-counts-4964.test.ts, run here).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PAGE="$ROOT/platform/api/public/loom/policy-counts.html"

@test "the page reads the counts same-origin for the gates in its address, never from localhost" {
  grep -q "fetch('/api/chorus/policies/counts?gate=' + encodeURIComponent(gate))" "$PAGE" || return 1
  grep -q "get('gate')" "$PAGE" || return 1
  ! grep -qE "localhost|127\.0\.0\.1|:33[0-9]{2}" "$PAGE" || return 1
}

@test "each row names its gate, its narrowing and its ceiling" {
  grep -q 'esc(p.gate)' "$PAGE" && grep -q "narrowed" "$PAGE" && grep -q "ceiling" "$PAGE" || return 1
}

@test "the route hands policy-validate only the asked gates' rows, with no single --gate" {
  ! grep -q "'policy-validate'), \['--gate', gate\]" "$ROOT/platform/api/src/server.ts" || return 1
  grep -q "gates.has(r.gate" "$ROOT/platform/api/src/handlers/policy-counts.ts" || return 1
}

@test "the handler's behaviour: several gates in one table, worst first; a bad gate list refused" {
  cd "$ROOT/platform/api"
  run npx jest --config '{"preset":"ts-jest","testEnvironment":"node"}' tests/policy-counts-4964.test.ts tests/policy-counts-4963.test.ts
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
  [[ "$output" == *"Tests:       16 passed, 16 total"* ]] || { echo "$output" | tail -5; return 1; }
}
