#!/usr/bin/env bats
# @test-type: integration
# @domain: security — the product domain this suite guards (#4334)
# #4180 — a row a person created refuses the crawler; once the crawler owns it,
# the same update is accepted. Proven live against a werk VARIANT, never prod:
# the door stamps ownedBy from the caller and only the owner may update.

# #4416 — with no variant named, the suite brings its own door (a private
# athena-make on a private in-memory dataset) instead of skipping every night.
setup_file() {
  if [ "${RUN_INTEGRATION:-}" = "true" ] && [ -z "${OWL_URL:-}" ]; then
    source "$BATS_TEST_DIRNAME/lib/test-store.sh"
    # the private copy has no Write permission for kade on the code graph, so
    # one principal here is refused where the crawler is let in
    export PRIVATE_DOOR_PREP='DELETE WHERE { GRAPH <urn:chorus:domains:security> { <https://jeffbridwell.com/chorus#permission-kade-code> ?p ?o } }'
    private_door || { export PRIVATE_DOOR_WHY="$TEST_STORE_WHY"; return 0; }
  fi
}
teardown_file() {
  source "$BATS_TEST_DIRNAME/lib/test-store.sh"
  private_door_stop
}
setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration — RUN_INTEGRATION=true against a werk variant"
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  OWL_URL="${OWL_URL:-}"
  [ -z "${PRIVATE_DOOR_WHY:-}" ] || skip "UNMEASURED — no private door: $PRIVATE_DOOR_WHY"
  case "$OWL_URL" in ""|*:3360*) skip "refuses to write to the canonical store — point OWL_URL at a werk variant" ;; esac
  TK="$("$REPO/platform/scripts/chorus-identity-token" kade 2>/dev/null)";    [ -n "$TK" ]
  TC="$("$REPO/platform/scripts/chorus-identity-token" crawler 2>/dev/null)"; [ -n "$TC" ]
  N="file-probe-4180-$$"
  BODY='{"filePath":"probe/4180/'$$'.rs","fileSha":"aaaa","hasKind":"code","hasLanguage":"rust"}'
}
teardown() { curl -s -o /dev/null -X DELETE -H "Authorization: Bearer $TC" --max-time 10 "$OWL_URL/code/files/$N" || true; }
put_as() { curl -s -o /dev/null -w '%{http_code}' -X PUT -H "Authorization: Bearer $1" -H 'Content-Type: application/json' --max-time 15 -d "$BODY" "$OWL_URL/code/files/$N"; }

@test "a row another principal owns opens to a Write permission, and refuses a principal without one" {
  # #4416 — rewritten for the #4185 rule, which this suite never saw because
  # it skipped every night: a Write Permission on the row's graph opens a row
  # another principal owns. The negative half needs a store whose permissions
  # the test may shape, so it runs only on the private door, where kade holds
  # no Write on urn:chorus:domains:code and the crawler does.
  [ -n "${PRIVATE_DOOR_DS:-}" ] || skip "UNMEASURED — the negative half needs a store without kade's permission; it runs only on the private door"
  # kade creates the row: kade owns it
  run curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $TK" -H 'Content-Type: application/json' --max-time 15 -d "[{\"name\":\"$N\",$(echo "$BODY" | cut -c2-)]" "$OWL_URL/code/files/batch"
  echo "kade create: $output"; [ "$output" = "201" ]
  # the crawler holds Write on the graph, so kade's row opens to it
  run put_as "$TC"; echo "crawler put on kade's row: $output"; [ "$output" = "200" ]
  # NEGATIVE (#3734): the crawler now owns a row; kade, with no Write
  # permission here, is refused on it
  N2="file-probe-4180b-$$"
  run curl -s -o /dev/null -w '%{http_code}' -X POST -H "Authorization: Bearer $TC" -H 'Content-Type: application/json' --max-time 15 -d "[{\"name\":\"$N2\",$(echo "$BODY" | sed 's/4180\//4180b\//' | cut -c2-)]" "$OWL_URL/code/files/batch"
  echo "crawler create: $output"; [ "$output" = "201" ]
  run curl -s -o /dev/null -w '%{http_code}' -X PUT -H "Authorization: Bearer $TK" -H 'Content-Type: application/json' --max-time 15 -d "$BODY" "$OWL_URL/code/files/$N2"
  echo "kade put on the crawler's row, no permission: $output"; [ "$output" = "403" ]
}
