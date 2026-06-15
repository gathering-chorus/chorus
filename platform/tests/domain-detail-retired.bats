#!/usr/bin/env bats
# #3351 retirement gate — the old hand-built Athena domain page is retired.
# The generated page (platform/api/public/domain.html + js/domain-renderer.js) supersedes it.
# This gate fails if the retired surface (or a link to it) comes back — structural memory,
# per the retirement-gate convention. CI-authoritative.

PUB="${CHORUS_HOME:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}/platform/api/public"

@test "old domain-detail.html is gone" {
  run test -e "$PUB/athena/domain-detail.html"
  [ "$status" -ne 0 ]
}

@test "old domain-detail.js is gone" {
  run test -e "$PUB/athena/domain-detail.js"
  [ "$status" -ne 0 ]
}

@test "nothing links to the retired domain-detail page" {
  # the old per-domain nav was domain-detail.html?id=<x>; everything routes to domain.html now
  run grep -rn "domain-detail\.html?id=" "$PUB"
  [ "$status" -ne 0 ]
}

@test "the generated domain page exists as the replacement" {
  [ -e "$PUB/domain.html" ]
  [ -e "$PUB/js/domain-renderer.js" ]
}
