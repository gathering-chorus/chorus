#!/usr/bin/env bats
# @test-type: unit — reads a test source file; no live service
#
# #4142 — platform/api's integration tier READS the live door. 27 of the
# 2026-09-11 13:01 run's 44 reds were POST/PUT/DELETE tests writing to the
# production graph as the nightly's principal (403 at the door; by hand as a
# role they mutate prod). Creates/updates/deletes are proven hermetically in
# tests/handlers/subdomain-entities.test.ts and friends. This guard keeps the
# writes from coming back.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  FILE="$ROOT/platform/api/tests/athena.integration.test.ts"
  CHECK="$ROOT/platform/tests/lib/no-live-writes.sh"
}

@test "athena.integration.test.ts has no write to a subdomain entity route" {
  run bash "$CHECK" "$FILE"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF: a POST to a subdomain route is caught" {
  tmp="$BATS_TEST_TMPDIR/bad.test.ts"
  cp "$FILE" "$tmp"
  printf '%s\n' "test('x', async () => { await fetch(\`\${API}/api/athena/subdomains/logs-domain/actors\`, { method: 'POST' }); });" >> "$tmp"
  run bash "$CHECK" "$tmp"
  [ "$status" -ne 0 ]
  [[ "$output" == *"subdomains/logs-domain/actors"* ]]
}

@test "NEGATIVE PROOF: a DELETE is caught too" {
  tmp="$BATS_TEST_TMPDIR/bad2.test.ts"
  cp "$FILE" "$tmp"
  printf '%s\n' "test('y', async () => { await fetch(\`\${API}/api/athena/subdomains/x/gaps/g1\`, { method: 'DELETE' }); });" >> "$tmp"
  run bash "$CHECK" "$tmp"
  [ "$status" -ne 0 ]
}

@test "control: POST /api/athena/validate (a pure validator) is allowed" {
  run bash "$CHECK" "$FILE"
  [ "$status" -eq 0 ]
  grep -q "api/athena/validate" "$FILE"
}
