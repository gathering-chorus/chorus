#!/usr/bin/env bats
# @test-type: integration — static page guards + live proxy reachability (skip-if-absent).
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

# It must render LIVE (via the runtime), not embed a static snapshot — that's the
# whole point. So it uses fetchJSON + the shared athena-flow runtime.
@test "page fetches live via athena-flow (fetchJSON, not a hardcoded snapshot)" {
  grep -q "athena-flow.js" "$PAGE"
  grep -q "fetchJSON" "$PAGE"
  grep -q "fetchFailed" "$PAGE"   # honest failure state, not a blank page
}

# It must census the real served set — assert the core model collections are queried.
@test "page censuses the core model collections" {
  for c in products domains services valuestreams valuestreamsteps chunks roles tests; do
    grep -q "'$c'" "$PAGE"
  done
}

# It surfaces version (the #3704 axis) and the populated-vs-empty worklist.
@test "page shows version + populated/empty status" {
  grep -qi "modelVersion" "$PAGE"
  grep -qiE "empty|populated" "$PAGE"
}

# Live: the same-origin /owl proxy the page uses actually serves a collection.
@test "the /owl proxy serves a collection (the path the page fetches)" {
  run curl -s --max-time 8 -o /dev/null -w '%{http_code}' "$API/owl/domains"
  [ "$status" -eq 0 ] || skip "chorus-api not reachable at $API"
  [ "$output" = "200" ]
  run curl -s --max-time 8 "$API/owl/domains"
  echo "$output" | grep -q '"count"'
  echo "$output" | grep -q '"modelVersion"'
}
