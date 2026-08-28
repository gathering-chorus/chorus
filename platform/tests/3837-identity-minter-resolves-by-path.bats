#!/usr/bin/env bats
# @test-type: unit — sources only the wrapper function; fixture minter + fixture DAL, no live service
# #3837 — the athena-model() shell wrapper minted via `command chorus-identity-token`,
# a bare-name lookup. The minter lives in platform/scripts, which role shells do
# not put on PATH, so the lookup was exit 127, the wrapper passed an EMPTY token,
# and every shell-session model write refused "identity-token-required" (wren ×11
# on 2026-08-27, silas ×4 on 08-28) while the identity server was fine.
# Negative proof (#3734): with the minter ABSENT the wrapper must say so on stderr,
# not silently hand the DAL nothing. Positive: with a minter at the path, the
# wrapper uses it even when PATH has no such command.

ENV_SETUP="$BATS_TEST_DIRNAME/../scripts/chorus-env-setup.sh"

extract_wrapper() {  # the function body only — sourcing the whole env-setup touches live state
  sed -n '/athena-model() {/,/^  }/p' "$ENV_SETUP"
}

setup() {
  export HOME_FIXTURE="$BATS_TEST_TMPDIR/home"
  mkdir -p "$HOME_FIXTURE/bin"
  # a stand-in DAL binary that prints the token it was handed
  printf '#!/bin/bash\necho "TOKEN=[${CHORUS_IDENTITY_TOKEN:-}]"\n' > "$HOME_FIXTURE/bin/athena-model"
  chmod +x "$HOME_FIXTURE/bin/athena-model"
}

@test "NEGATIVE PROOF: minter absent → wrapper warns on stderr and hands the DAL an empty token" {
  run bash -c "
    export PATH='$HOME_FIXTURE/bin:/usr/bin:/bin'
    export CHORUS_HOME='$BATS_TEST_TMPDIR/no-chorus' CHORUS_ROLE=silas DEPLOY_ROLE=silas
    $(extract_wrapper)
    athena-model add --kind decision --name x
  "
  [[ "$output" == *"WARN no identity token minted"* ]]
  [[ "$output" == *"TOKEN=[]"* ]]
}

@test "minter present at CHORUS_HOME/platform/scripts → used even though PATH has no such command" {
  mkdir -p "$BATS_TEST_TMPDIR/chorus/platform/scripts"
  printf '#!/bin/bash\necho "minted-for-$1"\n' > "$BATS_TEST_TMPDIR/chorus/platform/scripts/chorus-identity-token"
  chmod +x "$BATS_TEST_TMPDIR/chorus/platform/scripts/chorus-identity-token"
  run bash -c "
    export PATH='$HOME_FIXTURE/bin:/usr/bin:/bin'
    export CHORUS_HOME='$BATS_TEST_TMPDIR/chorus' CHORUS_ROLE=silas DEPLOY_ROLE=silas
    $(extract_wrapper)
    athena-model add --kind decision --name x
  "
  [[ "$output" == *"TOKEN=[minted-for-silas]"* ]]
  [[ "$output" != *"WARN"* ]]
}
