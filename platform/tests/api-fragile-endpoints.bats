#!/usr/bin/env bats
# @test-type: contract — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
# @domain: search — the product domain this suite guards (#4334)
load test_helper
# api-fragile-endpoints.bats — E2E tests for fragile API endpoints (#1776)
# What Jeff sees: endpoints that break silently — Athena 500s, completeness timeouts,
# seed webhook drops. These tests run against the real stack, no mocks.
# Contract-level assertions only: URL + response shape.

CHORUS_API="http://localhost:3340"
APP_API="http://localhost:3000"

setup() {
  # #4336: a live-only contract suite. When chorus-api does not answer, the
  # questions below are UNMEASURED, never red and never green.
  curl -s -o /dev/null --max-time 3 http://localhost:3340/api/chorus/health \
    || skip "UNMEASURED — chorus-api :3340 not answering (#4336)"
}


# --- AC 4 (deleted #4925, Wren): the signed-Twilio case wrote a real seed and read the app's
# secrets, so it was gated and skipped every night. The live seed path is probed daily by
# platform/scripts/seed-probe.sh (com.chorus.seed-probe, 05:55). ---

# --- AC 5: Chorus search returns results array ---

@test "GET /api/chorus/search returns results array" {
  result=$(curl -sf "$CHORUS_API/api/chorus/search?q=test&limit=3" 2>/dev/null)
  [ $? -eq 0 ]
  has_results=$(echo "$result" | python3 -c "import json,sys; d=json.load(sys.stdin); print('yes' if isinstance(d.get('results'), list) else 'no')" 2>/dev/null)
  [ "$has_results" = "yes" ]
}

@test "GET /api/chorus/search with empty query returns 400" {
  http_code=$(curl -s -o /dev/null -w "%{http_code}" "$CHORUS_API/api/chorus/search?q=&limit=3" 2>/dev/null)
  [ "$http_code" = "400" ]
}
