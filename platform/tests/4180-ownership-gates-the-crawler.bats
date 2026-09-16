#!/usr/bin/env bats
# @test-type: integration
# #4180 — a row a person created refuses the crawler; once the crawler owns it,
# the same update is accepted. Proven live against a werk VARIANT, never prod:
# the door stamps ownedBy from the caller and only the owner may update.

setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration — RUN_INTEGRATION=true against a werk variant"
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  OWL_URL="${OWL_URL:-}"
  case "$OWL_URL" in ""|*:3360*) skip "refuses to write to the canonical store — point OWL_URL at a werk variant" ;; esac
  TK="$("$REPO/platform/scripts/chorus-identity-token" kade 2>/dev/null)";    [ -n "$TK" ]
  TC="$("$REPO/platform/scripts/chorus-identity-token" crawler 2>/dev/null)"; [ -n "$TC" ]
  N="file-probe-4180-$$"
  BODY='{"filePath":"probe/4180/'$$'.rs","fileSha":"aaaa","hasKind":"code","hasLanguage":"rust"}'
}
teardown() { curl -s -o /dev/null -X DELETE -H "Authorization: Bearer $TC" --max-time 10 "$OWL_URL/code/files/$N" || true; }
put_as() { curl -s -o /dev/null -w '%{http_code}' -X PUT -H "Authorization: Bearer $1" -H 'Content-Type: application/json' --max-time 15 -d "$BODY" "$OWL_URL/code/files/$N"; }

@test "a person-owned row refuses the crawler; re-owned by the crawler it accepts" {
  # a PERSON creates the row
  run curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $TK" -H 'Content-Type: application/json' --max-time 15 -d "[{\"name\":\"$N\",$(echo "$BODY" | cut -c2-)]" "$OWL_URL/code/files/batch"
  [ "$output" = "201" ]
  # NEGATIVE half (#3734): the crawler's update is REFUSED — the state that
  # left the nightly red on 2026-09-15
  run put_as "$TC"; [ "$output" = "403" ]
  # hand-off: the owner deletes, the crawler creates, the crawler now updates
  run curl -s -o /dev/null -w '%{http_code}' -X DELETE -H "Authorization: Bearer $TK" --max-time 15 "$OWL_URL/code/files/$N"; [ "$output" = "200" ]
  run curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $TC" -H 'Content-Type: application/json' --max-time 15 -d "[{\"name\":\"$N\",$(echo "$BODY" | cut -c2-)]" "$OWL_URL/code/files/batch"
  [ "$output" = "201" ]
  run put_as "$TC"; [ "$output" = "200" ]
}
