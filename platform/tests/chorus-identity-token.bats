#!/usr/bin/env bats
# @test-type: integration — hermetic (own tmpdir identity dir + fake creds, no live CSS, no ~/.chorus) but shells the reader subprocess + probes a dead port, so integration not unit.
# #3690 — chorus-identity-token reader logic, hermetic (own identity dir, no live
# CSS, no ~/.chorus). The mint's forwarded-header contract is proven separately
# by the LIVE end-to-end (chorus-model resolving principal-silas). Here: role
# resolution, fail-closed exits (token NEVER on stdout on failure), cache TTL.
# Errors go to stderr; token to stdout — so fail-closed cases capture stdout-only.
setup() {
  READER="$BATS_TEST_DIRNAME/../scripts/chorus-identity-token"
  export CHORUS_IDENTITY_DIR="$BATS_TEST_TMPDIR/identity"
  mkdir -p "$CHORUS_IDENTITY_DIR/silas"
  cat > "$CHORUS_IDENTITY_DIR/silas/cred.json" <<JSON
{"agent":"silas","webId":"https://id.example/silas/profile/card#me","issuer":"https://id.example/","tokenEndpoint":"https://id.example/.oidc/token","id":"id_x","secret":"sec_x"}
JSON
}
_jwt() {  # $1 = exp epoch
  python3 - "$1" <<'PY'
import sys, json, base64
def b(o): return base64.urlsafe_b64encode(json.dumps(o).encode()).rstrip(b'=').decode()
print(b({"alg":"ES256"})+"."+b({"webid":"https://id.example/silas/profile/card#me","exp":int(sys.argv[1])})+".sig")
PY
}
# run the reader capturing STDOUT ONLY (errors are on stderr by contract)
stdout_only() { run bash -c "\"$READER\" \"\$@\" 2>/dev/null" _ "$@"; }

@test "no role (arg + envs unset) → rc=2" {
  run bash -c "unset DEPLOY_ROLE CHORUS_ROLE; \"$READER\" 2>/dev/null"
  [ "$status" -eq 2 ]
  [ -z "$output" ]
}

@test "missing cred → rc=3, nothing on stdout" {
  stdout_only ghost-role
  [ "$status" -eq 3 ]
  [ -z "$output" ]
}

@test "CSS unreachable + no cache → rc=5, NO token on stdout" {
  CHORUS_CSS_LOCAL="http://127.0.0.1:1" stdout_only silas
  [ "$status" -eq 5 ]
  [ -z "$output" ]
}

@test "fresh cached token served WITHOUT touching CSS" {
  tok="$(_jwt 9999999999)"
  printf '%s' "$tok" > "$CHORUS_IDENTITY_DIR/silas/token.cache"; chmod 600 "$CHORUS_IDENTITY_DIR/silas/token.cache"
  CHORUS_CSS_LOCAL="http://127.0.0.1:1" stdout_only silas
  [ "$status" -eq 0 ]
  [ "$output" = "$tok" ]
}

@test "EXPIRED cached token NOT served — falls through to mint (fail-closed here)" {
  printf '%s' "$(_jwt 1000000000)" > "$CHORUS_IDENTITY_DIR/silas/token.cache"
  CHORUS_CSS_LOCAL="http://127.0.0.1:1" stdout_only silas
  [ "$status" -eq 5 ]
  [ -z "$output" ]
}
