#!/usr/bin/env bats
# @test-type: e2e — after the land: the live public URL with no sign-in
# @domain: identity
# @card: #4985
#
# AC5 (after test): Jeff's browser with an expired session sees the sign-in page and nothing
# of the room. A request with no valid session (as an expired one is after #4966) is sent to
# sign in, and the room's API refuses it. Measured by Wren 16:39: 302 and 401.

setup() {
  PUBLIC="${PUBLIC_URL:-https://lightlifeurbangardens.com}"
}

verdict() {  # $1 page code, $2 page location, $3 api code
  [[ "$1" =~ ^30[1278]$ ]] && [[ "$2" == *"/_auth/"* ]] && [[ "$3" == "401" || "$3" == "403" || "$3" =~ ^30 ]]
}

@test "with no valid session the room page goes to sign-in and its API refuses" {
  page=$(curl -s -o /dev/null -w '%{http_code} %{redirect_url}' --max-time 15 -H 'Cookie: chorus_session=expired' "$PUBLIC/clearing-v2/")
  api=$(curl -s -o /dev/null -w '%{http_code}' --max-time 15 -H 'Cookie: chorus_session=expired' "$PUBLIC/clearing-v2/api/rooms/team/people")
  echo "page=$page api=$api"
  verdict "${page%% *}" "${page#* }" "$api"
}

@test "NEGATIVE PROOF: a room page served (200) to no session fails" {
  if verdict 200 "" 200; then return 1; fi
}
