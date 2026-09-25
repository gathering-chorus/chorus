#!/usr/bin/env bats
# @test-type: integration — NEGATIVE PROOFS (#3734) at the live Test door; every request is refused, nothing is written
# #4162 — one field, testType, says what kind of proving a Test is. The door
# must refuse a Test that does not carry it, and a Test that still carries a
# retired field (testConcern, pyramidLayer). Both are refusals: a 4xx creates
# no row, so this suite never writes production.
#
# Until the model with testType is deployed, the door still serves the old
# shape. That is a state this suite cannot measure, so it skips with the reason
# (UNMEASURED), never passes.

ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
API="${TEST_DOOR_API:-http://localhost:3360}"

setup() {
  curl -sf --max-time 5 "$API/health" >/dev/null 2>&1 || skip "UNMEASURED — $API not answering"
  TOKEN="$("$ROOT/platform/scripts/chorus-identity-token" kade 2>/dev/null)"
  [ -n "$TOKEN" ] || skip "UNMEASURED — no identity token for kade"
  curl -sf --max-time 10 "$API/tests/tests/openapi.json" 2>/dev/null | grep -q '"testType"' \
    || skip "UNMEASURED — the live Test shape does not serve testType yet (model not deployed)"
  NAME="zz-4162-probe-$$"
}

post() { # body → "<http code> <body>"
  curl -s --max-time 30 -o "$BATS_TEST_TMPDIR/body" -w '%{http_code}' -X POST \
    -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
    --data "$1" "$API/tests/tests"
}

base='"filePath":"platform/tests/zz-4162.bats","testName":"zz probe","inFile":"file-zz-4162","covers":"tests","hermeticity":"hermetic"'

@test "NEGATIVE PROOF: a Test with no testType is refused and the refusal names testType" {
  code="$(post "{\"name\":\"$NAME-a\",$base}")"
  cat "$BATS_TEST_TMPDIR/body"; echo
  [ "$code" = "422" ]
  grep -q "testType" "$BATS_TEST_TMPDIR/body"
}

@test "NEGATIVE PROOF: a Test carrying the retired testConcern is refused as off-model" {
  code="$(post "{\"name\":\"$NAME-b\",$base,\"testType\":\"security\",\"testConcern\":\"security\"}")"
  cat "$BATS_TEST_TMPDIR/body"; echo
  [ "$code" = "422" ]
  grep -q "testConcern" "$BATS_TEST_TMPDIR/body"
}

@test "NEGATIVE PROOF: a Test carrying the retired pyramidLayer is refused as off-model" {
  code="$(post "{\"name\":\"$NAME-c\",$base,\"testType\":\"unit\",\"pyramidLayer\":\"unit\"}")"
  cat "$BATS_TEST_TMPDIR/body"; echo
  [ "$code" = "422" ]
  grep -q "pyramidLayer" "$BATS_TEST_TMPDIR/body"
}

@test "NEGATIVE PROOF: a testType outside the set is refused" {
  code="$(post "{\"name\":\"$NAME-d\",$base,\"testType\":\"api\"}")"
  cat "$BATS_TEST_TMPDIR/body"; echo
  [ "$code" = "422" ]
}
