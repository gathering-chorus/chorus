#!/usr/bin/env bats
# @test-type: unit — drives platform/scripts/chorus-identity-token with a fixture credential; no CSS (the mint endpoint is a dead port).
# @domain: identity — the product domain this suite guards (#4334)
#
# #4202 — a role's credential bound to a macOS account is usable only by a
# process running as that account. Asserts are simple commands (bash 3.2 `[[`).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  S="$ROOT/platform/scripts/chorus-identity-token"
  T="$BATS_TEST_TMPDIR"; mkdir -p "$T/id/kade"
  export CHORUS_IDENTITY_DIR="$T/id" CHORUS_CSS_LOCAL="http://127.0.0.1:9" CHORUS_MINT_RETRIES=1
}
cred() { printf '{"id":"x","secret":"y","issuer":"https://id.example","webId":"https://id.example/kade/profile/card#me"%s}' "${1:-}" > "$T/id/kade/cred.json"; }

@test "NEGATIVE PROOF — a credential bound to ANOTHER account is refused (exit 6), no token on stdout" {
  cred ',"hostAccount":"chorus-kade"'
  run "$S" kade
  [ "$status" -eq 6 ]
  [ -z "$output" ] || ! printf '%s' "$output" | grep -q '^eyJ'
  printf '%s' "$output" | grep -q "bound to account 'chorus-kade'"
  printf '%s' "$output" | grep -q "runs as '$(id -un)'"
}

@test "NEGATIVE PROOF — a cached token does not bypass the account check" {
  cred ',"hostAccount":"chorus-kade"'
  printf 'eyJhbGciOiJFUzI1NiJ9.eyJleHAiOjk5OTk5OTk5OTl9.sig' > "$T/id/kade/token.cache"
  run "$S" kade
  [ "$status" -eq 6 ]
  ! printf '%s' "$output" | grep -q '^eyJ'
}

@test "a credential bound to THIS account passes the check (fails later at the dead mint endpoint, exit 5 not 6)" {
  cred ",\"hostAccount\":\"$(id -un)\""
  run "$S" kade
  [ "$status" -eq 5 ]
}

@test "an unbound credential (no hostAccount) is not checked (exit 5 at the dead mint endpoint)" {
  cred
  run "$S" kade
  [ "$status" -eq 5 ]
}
