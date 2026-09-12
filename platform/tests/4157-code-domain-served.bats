#!/usr/bin/env bats
# @test-type: integration — live athena-make + its store (RUN_INTEGRATION=true).
# #4157 — the code domain is served and writable through the generated API.
# Runs in prove-live against the werk variant (OWL_URL) and by hand against any
# athena-make (ATHENA_MAKE_URL). Routes are the bare collections: discovery
# advertises /v1/… but the server answers only the bare paths (measured 2026-09-12).

setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "RUN_INTEGRATION not set (integration test — live store + athena-make)"
  # Which athena-make is under test: prove-live hands the werk VARIANT as OWL_URL;
  # the nightly (WERK_TEST_NIGHTLY=1) measures canonical :3360. The werk TEST lane
  # runs before env-up, when no variant exists and canonical does not serve this
  # card's classes yet, so it is not measurable there: skip loudly, never fail on
  # the wrong server (this happened on run 1, 2026-09-12 16:18).
  if [ -n "${ATHENA_MAKE_URL:-${OWL_URL:-}}" ]; then URL="${ATHENA_MAKE_URL:-$OWL_URL}"
  elif [ "${WERK_TEST_NIGHTLY:-}" = "1" ]; then URL="http://localhost:3360"
  else skip "no OWL_URL/ATHENA_MAKE_URL and not the nightly: prove-live measures this against the variant"; fi
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  ROLE="${CHORUS_ROLE:-kade}"
  TOKEN="${CHORUS_IDENTITY_TOKEN:-$("$ROOT/platform/scripts/chorus-identity-token" "$ROLE" 2>/dev/null)}"
  SACRIFICE="code-domain-4157-sacrificial"
  BODY="$BATS_TEST_TMPDIR/body.json"
}

teardown() {
  [ -n "${URL:-}" ] && curl -s -o /dev/null -X DELETE -H "Authorization: Bearer ${TOKEN:-}" "$URL/codefiles/$SACRIFICE" || true
}

post_codefile() { # language → prints http code, body in $BODY
  curl -s -o "$BODY" -w '%{http_code}' -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -X POST \
    -d "{\"name\":\"$SACRIFICE\",\"filePath\":\"platform/tests/4157-code-domain-served.bats\",\"hasKind\":\"test\",\"hasLanguage\":\"$1\"}" \
    "$URL/codefiles"
}

@test "4157 AC2: discovery lists CodeFile, CodeKind and Language" {
  run curl -sf "$URL/"
  [ "$status" -eq 0 ]
  for k in CodeFile CodeKind Language; do echo "$output" | grep -q "\"kind\": \"$k\""; done
}

@test "4157 AC2: the deploy left every kind and language answering GET (designing/data/code-vocab.ttl)" {
  for k in code test log config doc; do
    [ "$(curl -s -o /dev/null -w '%{http_code}' "$URL/codekinds/$k")" = 200 ]
  done
  for l in rust typescript bash python markdown turtle; do
    [ "$(curl -s -o /dev/null -w '%{http_code}' "$URL/languages/$l")" = 200 ]
  done
}

@test "4157 AC3 negative proof: a free-string language is refused as unknown-target and nothing is written" {
  code=$(post_codefile klingon)
  # the refusal must be the DAL's referential one, not a 404 for a wrong URL
  ! grep -q 'unknown route' "$BODY"
  grep -q 'unknown-target' "$BODY"
  [ "$code" = 422 ] || [ "$code" = 404 ]
  [ "$(curl -s -o /dev/null -w '%{http_code}' "$URL/codefiles/$SACRIFICE")" != 200 ]
}

@test "4157 AC2: a well-formed CodeFile creates and reads back with kind and language" {
  code=$(post_codefile bash)
  [ "$code" = 201 ] || [ "$code" = 200 ]
  run curl -s "$URL/codefiles/$SACRIFICE"
  echo "$output" | grep -q '4157-code-domain-served.bats'
  echo "$output" | grep -q '"hasKind"'
  echo "$output" | grep -q '"hasLanguage"'
}
