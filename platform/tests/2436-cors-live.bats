#!/usr/bin/env bats
# @test-type: e2e:security — probes the deployed chorus-api over HTTP (the "api" signal); a machine test, nightly only
# @domain: security — the product domain this suite guards (#4334)
# @needs: machine — scores the deployed chorus-api; a card run leaves it to the nightly (#4539)
#
# #2436 live leg, moved here by #4925. It was opt-in (CORS_SUT) so a card's run never
# scored a box that had not deployed its change: against such a host it is correctly
# red and would deadlock a land on its own gate. As a machine test it runs only in the
# nightly, after the land, against prod chorus-api: read-only (it asks with a stranger
# Origin and with our own), so it needs no write gate. It skipped every night before.

setup() {
  PROBE="${BATS_TEST_DIRNAME}/../scripts/cors-coverage.sh"
  CORS_SUT="${CORS_SUT:-http://localhost:3340}"
}

@test "live: prod chorus-api refuses a stranger and serves our own pages" {
  run env CHORUS_API_URL="$CORS_SUT" bash "$PROBE"
  echo "$output"
  [ "$status" -eq 0 ]
}
