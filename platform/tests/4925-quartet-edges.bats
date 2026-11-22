#!/usr/bin/env bats
# @test-type: unit — quartet-walk's resolve_edge and patterned_value over a stubbed curl, store and generator
# @domain: tests
# @card: #4925 · owner: kade
# Round 2 (review of round 1): the store fallback in resolve_edge and the as-is rule in
# patterned_value had no test through the walk's own functions, only tagged_match with the
# flag handed in. These run the real functions with curl, the store and athena-gen stubbed.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
WALK="$ROOT/platform/tests/lib/quartet-walk.sh"

setup() {
  for f in resolve_edge patterned_value tagged_match; do
    eval "$(sed -n '/^'"$f"'() {/,/^}/p' "$WALK")"
    declare -F "$f" >/dev/null || { echo "$f is gone from quartet-walk.sh"; return 1; }
  done
  WORK="$BATS_TEST_TMPDIR"; API="http://api.invalid"; RESIDUE_QUERY="http://store.invalid/query"
  # athena-gen stub: one GET route for any class
  GEN="$BATS_TEST_TMPDIR/gen"
  printf '#!/bin/sh\necho %s\n' "'{\"unit\":{\"routes\":[\"GET /things/things\"]}}'" > "$GEN"; chmod +x "$GEN"
  # curl stub: -G is the store; anything else is the API (body from API_BODY, code from API_CODE)
  mkdir -p "$BATS_TEST_TMPDIR/bin"
  cat > "$BATS_TEST_TMPDIR/bin/curl" <<'SH'
#!/bin/bash
empty='{"results":{"bindings":[]}}'
for a in "$@"; do [ "$a" = "-G" ] && { printf '%s' "${STORE_JSON:-$empty}"; exit 0; }; done
out=""; prev=""
for a in "$@"; do [ "$prev" = "-o" ] && out="$a"; prev="$a"; done
[ -n "$out" ] && printf '%s' "${API_BODY:-}" > "$out" || printf '%s' "${API_BODY:-}"
case "$*" in *%\{http_code\}*) printf '%s' "${API_CODE:-200}" ;; esac
SH
  chmod +x "$BATS_TEST_TMPDIR/bin/curl"
  PATH="$BATS_TEST_TMPDIR/bin:$PATH"
}

@test "resolve_edge: a route that 404s falls back to a real row in the store" {
  API_CODE=404 API_BODY='{"error":"unknown route"}' \
  STORE_JSON='{"results":{"bindings":[{"s":{"value":"https://jeffbridwell.com/chorus#hemenway-creativity-is-the-limit"}}]}}' \
    run resolve_edge Principle
  [ "$output" = "hemenway-creativity-is-the-limit" ] || { echo "$output"; return 1; }
  [ ! -s "$WORK/edge-why" ] || { echo "a found row still wrote a why: $(cat "$WORK/edge-why")"; return 1; }
}

@test "NEGATIVE PROOF: no route row and no store row is empty, with the dead route named" {
  API_CODE=404 API_BODY='{}' run resolve_edge Principle
  [ -z "$output" ] || { echo "invented a row: $output"; return 1; }
  grep -q "Principle: route /things/things answered 404, and the store holds no Principle row" "$WORK/edge-why"
}

@test "control: a route that serves a row is used, the store is not asked" {
  API_CODE=200 API_BODY='{"data":[{"name":"served-row"}]}' STORE_JSON='{"results":{"bindings":[{"s":{"value":"x#store-row"}}]}}' \
    run resolve_edge Principle
  [ "$output" = "served-row" ] || { echo "$output"; return 1; }
}

# patterned_value asks the store for the pattern and the unique count (two values, one line)
pv_store() { # pv_store <pattern> <unique count>
  printf '{"results":{"bindings":[{"p":{"value":"%s"},"u":{"value":"%s"}}]}}' "$1" "$2"
}

@test "patterned_value: a non-unique, non-name, non-anyURI field takes the served value as-is" {
  STORE_JSON="$(pv_store '^0[0-7]{3}$' 0)" API_BODY='{"data":[{"fileMode":"0644"}]}' \
    run patterned_value DeployTarget fileMode zz-4267-r1 string
  [ "$output" = "0644" ] || { echo "$output"; return 1; }
}

@test "NEGATIVE PROOF: unique, name and anyURI fields never take a served value as-is" {
  STORE_JSON="$(pv_store '^0[0-7]{3}$' 1)" API_BODY='{"data":[{"fileMode":"0644"}]}' run patterned_value DeployTarget fileMode t string
  [ "$output" = 'UNMET ^0[0-7]{3}$' ] || { echo "unique field: $output"; return 1; }
  STORE_JSON="$(pv_store '^0[0-7]{3}$' 0)" API_BODY='{"data":[{"name":"0644"}]}' run patterned_value DeployTarget name t string
  [ "$output" = 'UNMET ^0[0-7]{3}$' ] || { echo "name field: $output"; return 1; }
  STORE_JSON="$(pv_store '^0[0-7]{3}$' 0)" API_BODY='{"data":[{"fileMode":"0644"}]}' run patterned_value DeployTarget fileMode t anyURI
  [ "$output" = 'UNMET ^0[0-7]{3}$' ] || { echo "anyURI field: $output"; return 1; }
}
