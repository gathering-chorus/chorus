#!/usr/bin/env bats
# @test-type: unit — hermetic: a fixture credential dir, the script's --check (renders to a temp file, never the live shiro.ini)
# @domain: persistence
# @card: #4716
# @owner: kade
# #4716 AC2 — platform/scripts/fuseki-shiro-deploy.sh mints one credential per
# writer (dal, deploy) beside the shared one, 0600, once: a second run keeps the
# first value, so the store keeps naming the same writer.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  SCRIPT="$ROOT/platform/scripts/fuseki-shiro-deploy.sh"
  D="$BATS_TEST_TMPDIR/cred"
  mkdir -p "$D"
  printf '%s=%s\n' FUSEKI_ADMIN_PASSWORD fixture-shared > "$D/fuseki-write.env"
  export FUSEKI_WRITE_ENV="$D/fuseki-write.env" FUSEKI_SHIRO_RUNTIME="$BATS_TEST_TMPDIR/shiro.ini"
}

@test "#4716: --check mints a 0600 dal and deploy credential and renders no placeholder" {
  run bash "$SCRIPT" --check
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  for w in dal deploy; do
    [ -s "$D/fuseki-$w.env" ] || return 1
    [ "$(stat -f '%Lp' "$D/fuseki-$w.env")" = "600" ] || return 1
  done
  [ ! -e "$BATS_TEST_TMPDIR/shiro.ini" ] || return 1   # a check never writes the live config
}

@test "#4716: a second run keeps each writer's first credential" {
  bash "$SCRIPT" --check >/dev/null 2>&1
  first="$(shasum "$D/fuseki-dal.env" "$D/fuseki-deploy.env")"
  bash "$SCRIPT" --check >/dev/null 2>&1
  [ "$(shasum "$D/fuseki-dal.env" "$D/fuseki-deploy.env")" = "$first" ] || return 1
}

@test "#4716 NEGATIVE PROOF: no place to mint a writer credential refuses, fail-closed" {
  chmod 500 "$D"
  run bash "$SCRIPT" --check
  chmod 700 "$D"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"could not mint the dal/deploy credentials"* ]] || { echo "$output"; return 1; }
}
