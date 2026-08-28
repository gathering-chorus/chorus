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
  offenders=$(grep -rlnE --exclude-dir={node_modules,target,dist,coverage,logs} "[0-9]+/api/nudge" platform 2>/dev/null \
    | grep -vE 'node_modules|/dist|/coverage|\.test\.|tests/|\.bats$|\.map$|\.html$' \
    | grep -vE '/target/|platform/logs/' \
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

@test "the nightly coverage tier does not POST pulse directly" {
  cd "$REPO"
  # #3734 — retargeted off the standalone nightly-coverage script, which was RETIRED
  # as an orphan (nothing invoked it; #3527 folded the coverage tier into
  # nightly-suites.sh and left the original standing). Note the failure mode this
  # test had: `grep -c` against a DELETED file exits 2 with empty output, so
  # `[ "$output" -eq 0 ]` errors rather than passing — but a variant that compared
  # differently would have gone GREEN on a file that no longer exists, which is
  # the same can't-tell-two-states shape this card is about. Point it at the live
  # implementation so it guards something real.
  run grep -cE "3475/api/nudge" platform/scripts/nightly-suites.sh
  [ "$output" -eq 0 ]
}

@test "transport.ts error-notify does not POST pulse directly" {
  cd "$REPO"
  run grep -cE "[0-9]+/api/nudge" platform/mcp-server/src/transport.ts
  [ "$output" -eq 0 ]
}

@test "NEGATIVE: a SOURCE file referencing the nudge URL is still caught (#3949)" {
  # The exclusions above dropped compiled binaries (target/) and runtime logs —
  # scan artifacts, not sources (the #3915 binary-in-a-source-scan class). This
  # proves the narrowing did not blind the sweep to real code.
  cd "$REPO"
  tmp="platform/probe-3949-nudge-url.sh"
  echo 'curl http://localhost:3475/api/nudge' > "$tmp"
  offenders=$(grep -rlnE --exclude-dir={node_modules,target,dist,coverage,logs} "[0-9]+/api/nudge" platform 2>/dev/null \
    | grep -vE 'node_modules|/dist|/coverage|\.test\.|tests/|\.bats$|\.map$|\.html$' \
    | grep -vE '/target/|platform/logs/' \
    | grep -vE 'platform/pulse/' \
    | grep -vE 'platform/mcp-server/src/server\.ts$' || true)
  rm -f "$tmp"
  echo "$offenders" | grep -q "probe-3949-nudge-url.sh"
}
