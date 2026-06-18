#!/usr/bin/env bats
# @test-type: unit — static source guard (greps for forbidden pulse URLs); no live server
# #3485 — one execution path: the MCP's executeNudge is the SOLE poster to
# pulse /api/nudge. Every other sender (ops-nudge, nightly-coverage, the #3001
# error-notify in transport.ts) must route THROUGH the MCP, not POST pulse
# directly. Guard mirrors test-hardcoded-bin-paths.
#
# Jeff 2026-06-18: "i dont want 2 ways to call nudge" / "all of them must
# point to mcp".
#
# Keys on the URL FORM "3475/api/nudge" (an actual pulse endpoint URL), so a
# descriptive string like chorus-crawl's "POST /api/nudge (3475)" is NOT a
# false positive — only real POST targets match.

setup() {
  REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
}

# Only pulse (owns the route) and mcp-server/src/server.ts (executeNudge, the
# single execution path) may reference the pulse nudge URL.
@test "only the MCP path references the pulse nudge URL (3475/api/nudge)" {
  cd "$REPO"
  offenders=$(grep -rlnE "[0-9]+/api/nudge" platform 2>/dev/null \
    | grep -vE 'node_modules|/dist|/coverage|\.test\.|tests/|\.bats$|\.map$|\.html$' \
    | grep -vE 'platform/pulse/' \
    | grep -vE 'platform/mcp-server/src/server\.ts$' \
    || true)
  if [ -n "$(echo "$offenders" | tr -d '[:space:]')" ]; then
    echo "Files referencing the pulse nudge URL outside the MCP path:" >&2
    echo "$offenders" | sed 's/^/  - /' >&2
    echo "Route these through the mcp-server /nudge endpoint (executeNudge)." >&2
    return 1
  fi
}

@test "ops-nudge targets the MCP nudge endpoint, not pulse" {
  cd "$REPO"
  run grep -cE "[0-9]+/api/nudge" platform/scripts/ops-nudge
  [ "$output" -eq 0 ]
  run grep -cE "/nudge" platform/scripts/ops-nudge
  [ "$output" -gt 0 ]
}

@test "nightly-coverage does not POST pulse directly" {
  cd "$REPO"
  run grep -cE "3475/api/nudge" platform/scripts/nightly-coverage.sh
  [ "$output" -eq 0 ]
}

@test "transport.ts error-notify does not POST pulse directly" {
  cd "$REPO"
  run grep -cE "[0-9]+/api/nudge" platform/mcp-server/src/transport.ts
  [ "$output" -eq 0 ]
}
