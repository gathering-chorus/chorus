#!/usr/bin/env bats
# @test-type: contract — the page and the route's handler; no server, no store.
# @domain: policies
# @card: #4963
# @owner: silas
#
# #4963 AC4 — the demo shows one table of the 13 with today's count each, worst first, on a
# page Jeff can open from his phone (not localhost): loom/policy-counts.html, served by
# chorus-api from disk, reads /api/chorus/policies/counts same-origin. The handler's own
# behaviour (worst first, narrowed marked, door rows only, 503 on a dead door, could-not-
# measure never shown as 0) is policy-counts-4963.test.ts, run here. The live answer on the
# LAN after the land is 4963-arch-policy-table-live.bats.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PAGE="$ROOT/platform/api/public/loom/policy-counts.html"

@test "the page reads the counts same-origin, never from localhost or another port" {
  grep -q "fetch('/api/chorus/policies/counts?gate=" "$PAGE" || return 1
  ! grep -qE "localhost|127\.0\.0\.1|:33[0-9]{2}|:3360" "$PAGE" || { grep -nE "localhost|127\.0\.0\.1|:33[0-9]{2}" "$PAGE"; return 1; }
}

@test "the page fits a phone and names each rule's narrowing and its ceiling" {
  grep -q 'name="viewport" content="width=device-width' "$PAGE" || return 1
  grep -q "narrowed" "$PAGE" && grep -q "ceiling" "$PAGE" && grep -q "could not measure" "$PAGE" || return 1
  grep -q "timeZone: 'America/New_York'" "$PAGE" || return 1
}

@test "chorus-api serves the route and the page from this tree" {
  grep -q "app.get('/api/chorus/policies/counts'" "$ROOT/platform/api/src/server.ts" || return 1
  grep -q "fetchPolicyCounts" "$ROOT/platform/api/src/server.ts" || return 1
}

@test "the handler's behaviour: worst first, narrowed marked, door rows only, 503 on a dead door" {
  cd "$ROOT/platform/api"
  run npx jest tests/policy-counts-4963.test.ts
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
  [[ "$output" == *"Tests:       10 passed, 10 total"* ]] || { echo "$output" | tail -5; return 1; }
}
