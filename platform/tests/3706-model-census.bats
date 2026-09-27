#!/usr/bin/env bats
# @test-type: integration:ui — page-exists guard + live proxy reachability (skip-if-absent).
# @domain: knowledge — the product domain this suite guards (#4334)
load test_helper
#
# #3706 — the live model census page. What Jeff sees: one page, "what is our model
# right now" — every collection owl-api serves, its live row count + version (#3704),
# fetched on load so it can't go stale (the static-snapshot problem behind "I feel
# blind"). Hand-authored on the athena-flow runtime; served from disk (no deploy).

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PAGE="$REPO/platform/api/public/athena/model.html"
  API="${API_URL:-http://localhost:3340}"
}

@test "model.html exists" { [ -f "$PAGE" ]; }

# Live: the same-origin /owl proxy the page uses actually serves a collection.
@test "the /owl proxy serves a collection (the path the page fetches)" {
  run curl -s --max-time 8 -o /dev/null -w '%{http_code}' "$API/owl/domains"
  [ "$status" -eq 0 ] || skip "chorus-api not reachable at $API"
  [ "$output" = "200" ]
  run curl -s --max-time 8 "$API/owl/domains"
  echo "$output" | grep -q '"count"'
  echo "$output" | grep -q '"modelVersion"'
}
