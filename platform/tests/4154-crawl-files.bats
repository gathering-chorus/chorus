#!/usr/bin/env bats
# @test-type: integration — live athena-make + its store (RUN_INTEGRATION=true)
# #4154 B2 — the one walker persists files THROUGH the generated API.
# Proves the lifecycle Jeff asked for: a file appears, changes, and leaves.

setup() {
  [ "${RUN_INTEGRATION:-}" = "true" ] || skip "RUN_INTEGRATION not set (integration test — live store + athena-make)"
  URL="${ATHENA_MAKE_URL:-${OWL_URL:-http://localhost:3360}}"
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  ROLE="${CHORUS_ROLE:-kade}"
  TOKEN="${CHORUS_IDENTITY_TOKEN:-$("$ROOT/platform/scripts/chorus-identity-token" "$ROLE" 2>/dev/null)}"
  TREE="$BATS_TEST_TMPDIR/tree"; mkdir -p "$TREE/platform/tests"
  FAKE="$TREE/platform/tests/zz-4154-fixture.bats"
  NAME=$(python3 -c "import hashlib,sys; print(hashlib.sha1(sys.argv[1].encode()).hexdigest())" "$FAKE")
  export CHORUS_IDENTITY_TOKEN="$TOKEN"
  walk() { ATHENA_MAKE_URL="$URL" CHORUS_ROOT="$TREE" CRAWL_BATCH=50 \
             python3 "$ROOT/platform/scripts/crawl-files.py" 2>&1; }
  row_code() { curl -s -o /dev/null -w '%{http_code}' "$URL/codefiles/$NAME"; }
}

teardown() {
  [ -n "${NAME:-}" ] && curl -s -o /dev/null -X DELETE -H "Authorization: Bearer ${TOKEN:-}" "$URL/codefiles/$NAME" || true
}

@test "a test file on disk becomes a row with kind=test, through the API, in the code domain graph" {
  printf '@test "x" {\n  true\n}\n' > "$FAKE"
  run walk
  [ "$status" -eq 0 ]
  [ "$(row_code)" = 200 ]
  run curl -s "$URL/codefiles/$NAME"
  echo "$output" | grep -q '"servedFrom": "urn:chorus:domains:code"'
  echo "$output" | grep -q 'code-kind-test'
  echo "$output" | grep -q 'language-bash'
  echo "$output" | grep -q 'zz-4154-fixture.bats'
}

@test "a second walk rewrites nothing — unchanged files are not touched (ADR-033: no storm)" {
  printf '@test "x" {\n  true\n}\n' > "$FAKE"
  walk
  run walk
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE '0 new, 0 replaced, [0-9]+ unchanged'
}

@test "NEGATIVE PROOF: the file leaves disk and the row leaves the graph" {
  printf '@test "x" {\n  true\n}\n' > "$FAKE"
  walk
  [ "$(row_code)" = 200 ]
  rm "$FAKE"
  run walk
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE '[1-9][0-9]* orphan\(s\) deleted'
  [ "$(row_code)" != 200 ]
}

@test "NEGATIVE PROOF: a file the model has no kind or language for is skipped, never written as 'code'" {
  printf 'x' > "$TREE/platform/tests/zz-4154.bin"
  run walk
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE 'skipped=[1-9]'
  N=$(python3 -c "import hashlib,sys; print(hashlib.sha1(sys.argv[1].encode()).hexdigest())" "$TREE/platform/tests/zz-4154.bin")
  [ "$(curl -s -o /dev/null -w '%{http_code}' "$URL/codefiles/$N")" != 200 ]
}
