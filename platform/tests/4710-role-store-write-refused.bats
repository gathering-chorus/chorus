#!/usr/bin/env bats
# @test-type: unit — a role cannot write the store with the shared admin login
# @domain: security
# @card: #4710 · owner: wren
# What Jeff sees: no role can change the store behind the door's back. #4716 folded into
# #4710 AC1 (Jeff 10-25 06:12): the shared admin login lives in the store owner's
# fuseki-write.env, and a role must neither read it nor write without it. Measured as
# chorus-wren 11-11 17:19: the file is 0600 jeffbridwell and unreadable, and an
# anonymous no-op update answers 401.
load test_helper
OWNER="$(stat -f '%Su' "$CHORUS_ROOT")"
CRED="${FUSEKI_WRITE_ENV:-$(eval echo "~$OWNER")/.gathering/data/fuseki-write.env}"
STORE="${FUSEKI_UPDATE_URL:-http://localhost:3030/pods/update}"

# prints why credential file $1 would let another account write the store, if it would
problems() {
  [ -f "$1" ] || { echo "$1: missing (unmeasured)"; return 0; }
  local mode; mode="$(stat -f '%Lp' "$1")"
  (( (8#$mode & 8#077) == 0 )) || echo "$1: group or other may read the store login (mode $mode)"
  return 0
}

@test "AC1(b): the shared store login is readable by its owner alone" {
  out="$(problems "$CRED")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC1(b) negative proof: a 0644 login file (any account reads it, the write gets through) is caught" {
  f="$BATS_TEST_TMPDIR/fuseki-write.env"; : > "$f"; chmod 644 "$f"
  out="$(problems "$f")"
  [[ "$out" == *"group or other may read the store login (mode 644)"* ]] || { echo "$out"; return 1; }
}

@test "AC1(b): a write with no login is refused by the store (a no-op update, nothing changes)" {
  code="$(curl -s -m5 -o /dev/null -w '%{http_code}' -X POST --data-urlencode 'update=INSERT DATA {}' "$STORE")"
  [ "$code" = 401 ] || [ "$code" = 403 ] || { echo "the store answered $code to a write with no login"; return 1; }
}
