#!/usr/bin/env bats
# @test-type: contract — reads the card's story (comments included) from chorus-api; no writes
# @domain: roles
# @card: #4985
#
# AC1: the cause of a logged-in role's tile disappearing is named on the card with evidence,
# before any code. Measured 16:48 (Silas): pulse read a quiet pane not live after 30 minutes
# (actor-state.ts deriveSessions), the role went 'off', Clearing v2 filtered the tile.

setup() {
  API="${CHORUS_API_URL:-http://localhost:3340}"
}

named() {  # $1 = the card's story: a measured time, the code that ends it, the rule, the filter
  # (the story keeps a comment's first 500 characters; the cause and its reads lead the comment)
  printf '%s\n' "$1" | grep -qE 'measured [0-9]{1,2}:[0-9]{2}' &&
  printf '%s\n' "$1" | grep -q 'actor-state.ts' &&
  printf '%s\n' "$1" | grep -q 'THIRTY_MINUTES_MS' &&
  printf '%s\n' "$1" | grep -q 'isRoleLoggedIn'
}

@test "the card names the cause (pulse's 30-minute quiet rule) with its reads" {
  run curl -sf --max-time 15 "$API/api/chorus/card-story/4985"
  [ "$status" -eq 0 ]
  named "$output"
}

@test "NEGATIVE PROOF: a card that only says 'tiles disappear' does not pass" {
  if named "Jeff: role tiles just disappear from clearing V2. Earlier presence fixes did not hold."; then return 1; fi
}
