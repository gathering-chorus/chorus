#!/usr/bin/env bats
# @test-type: unit — runs role-post with every input stubbed; no live service
# @domain: identity
# @card: #4925 · owner: kade
# Silas 11-22 10:19: role-login-persists test 4 set POST_HOME=$tmp but not POST_STATE_DIR,
# and role-post wrote its result into the REAL $HOME/.chorus/post, overwriting silas's live
# self-test reading (role-post-all then showed silas signin FAIL from a bats temp path).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

post() { # post <post-home> <real-home>
  env -u POST_STATE_DIR HOME="$2" POST_HOME="$1" POST_ROLE=silas POST_ACCOUNT=chorus-silas \
    POST_IDENTITY_DIR="$1/identity" POST_PRINCIPALS_CMD='echo {}' POST_REACH_CMD=true \
    bash "$ROOT/platform/scripts/role-post" >/dev/null 2>&1 || true
}

@test "NEGATIVE PROOF: with POST_HOME set, role-post leaves the real home's post dir untouched" {
  T="$BATS_TEST_TMPDIR/posthome"; R="$BATS_TEST_TMPDIR/realhome"; mkdir -p "$T" "$R"
  post "$T" "$R"
  [ ! -e "$R/.chorus/post" ] || { ls -la "$R/.chorus/post"; echo "wrote into the real home"; return 1; }
  [ -f "$T/.chorus/post/silas.result" ] || { echo "no result under POST_HOME"; return 1; }
}

@test "control: with no POST_HOME the state stays in HOME, as before" {
  R="$BATS_TEST_TMPDIR/realhome2"; mkdir -p "$R"
  env -u POST_STATE_DIR -u POST_HOME HOME="$R" POST_ROLE=silas POST_ACCOUNT=chorus-silas \
    POST_IDENTITY_DIR="$R/identity" POST_PRINCIPALS_CMD='echo {}' POST_REACH_CMD=true \
    bash "$ROOT/platform/scripts/role-post" >/dev/null 2>&1 || true
  [ -f "$R/.chorus/post/silas.result" ]
}
