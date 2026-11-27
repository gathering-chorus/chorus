#!/usr/bin/env bats
# @test-type: integration
# @domain: domains — the product domain this suite guards (#4334)
# #4522 — a Document needs its Domains (hasDomain) and a Domain needs its Document
# (hasDesignDoc). Through the per-class routes neither can go first: both answered
# 422 unknown-target on the live door, 2026-10-14 14:59. POST /create-linked writes
# both in one DAL batch. Proven on a private door (its own in-memory dataset), never prod.

setup_file() {
  if [ "${RUN_INTEGRATION:-}" = "true" ] && [ -z "${OWL_URL:-}" ]; then
    source "$BATS_TEST_DIRNAME/lib/test-store.sh"
    # the private copy holds no value-stream steps; a Domain needs one (atStep)
    export PRIVATE_DOOR_PREP="PREFIX c: <https://jeffbridwell.com/chorus#> INSERT DATA { GRAPH <urn:chorus:domains:value-streams> { c:value-stream-step-probe-4522 a c:ValueStreamStep } }"
    private_door || { export PRIVATE_DOOR_WHY="$TEST_STORE_WHY"; return 0; }
  fi
}
teardown_file() {
  source "$BATS_TEST_DIRNAME/lib/test-store.sh"
  private_door_stop
}
setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "integration — RUN_INTEGRATION=true"
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  [ -z "${PRIVATE_DOOR_WHY:-}" ] || skip "UNMEASURED — no private door: $PRIVATE_DOOR_WHY"
  [ -n "${PRIVATE_DOOR_DS:-}" ] || skip "UNMEASURED — this suite writes, so it runs only on its private door"
  case "${OWL_URL:-}" in ""|*:3360*) skip "refuses to write to the canonical store" ;; esac
  TS="$("$REPO/platform/scripts/chorus-identity-token" silas 2>/dev/null)"; [ -n "$TS" ]
  D="doc-probe-4522-$$"; M="domain-probe-4522-$$"
  DOC='{"name":"'$D'","label":"Probe doc","docTitle":"Probe doc","docHref":"/probe.html","docState":"draft","comment":"probe","hasDomain":["'$M'"]}'
  # #5045 — typed status and gaps leave the Domain shape. The private door runs the model that is
  # DEPLOYED, so before that deploy it still requires both ("'gaps' is required by the shape", cicd
  # run 3 of #5045) and after it refuses them as off-model. The body therefore carries exactly
  # what the door's own published contract requires of a new Domain, read here, never assumed.
  # A contract that cannot be read stops the test: a guess would pass or fail for the wrong reason.
  REQUIRED="$(curl -s --max-time 15 "$OWL_URL/domains/domains/openapi.json" | domain_create_required)"
  [ -n "$REQUIRED" ] || { echo "could not read DomainCreate.required from $OWL_URL/domains/domains/openapi.json"; return 1; }
  DOM='{"name":"'$M'","label":"probe","comment":"probe","purpose":"probe","atStep":"probe-4522","hasDesignDoc":"'$D'"'"$(typed_fields "$REQUIRED")"'}'
}
# the fields the door's contract requires of a new Domain, one per line (empty when it cannot be read)
domain_create_required() { python3 -c 'import sys, json
try: print("\n".join(json.load(sys.stdin)["components"]["schemas"]["DomainCreate"]["required"]))
except Exception: pass'; }
# ,"gaps":"none" and ,"status":"building" for whichever of the two the contract still requires
typed_fields() {
  out=""
  echo "$1" | grep -qx gaps && out="$out"',"gaps":"none"'
  echo "$1" | grep -qx status && out="$out"',"status":"building"'
  printf '%s' "$out"
}
post() { curl -s -o "$BATS_TEST_TMPDIR/body" -w '%{http_code}' -X POST -H "Authorization: Bearer $TS" -H "Content-Type: $1" --max-time 30 --data-binary "$2" "$OWL_URL/$3"; }

@test "NEGATIVE PROOF: alone, neither row can be created first" {
  run post application/json "$DOC" knowledge/documents
  echo "document alone: $output $(cat "$BATS_TEST_TMPDIR/body")"; [ "$output" = "422" ]
  grep -q unknown-target "$BATS_TEST_TMPDIR/body"
  run post application/json "$DOM" domains/domains
  echo "domain alone: $output $(cat "$BATS_TEST_TMPDIR/body")"; [ "$output" = "422" ]
  grep -q unknown-target "$BATS_TEST_TMPDIR/body"
}

@test "together, create-linked writes both, and each reads back with its link" {
  body="$(printf '/knowledge/documents\t%s\n/domains/domains\t%s\n' "$DOC" "$DOM")"
  run post text/plain "$body" create-linked
  echo "create-linked: $output $(cat "$BATS_TEST_TMPDIR/body")"; [ "$output" = "201" ]
  run curl -s --max-time 15 "$OWL_URL/knowledge/documents/$D"
  echo "$output" | grep -q "$M"
  run curl -s --max-time 15 "$OWL_URL/domains/domains/$M"
  echo "$output" | grep -q "$D"
}

@test "NEGATIVE PROOF: an unsigned linked create is refused before any row is read" {
  body="$(printf '/knowledge/documents\t%s\n' "$DOC")"
  run curl -s -o /dev/null -w '%{http_code}' -X POST --max-time 15 --data-binary "$body" "$OWL_URL/create-linked"
  [ "$output" = "401" ]
}
