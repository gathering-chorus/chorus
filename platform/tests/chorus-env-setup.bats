#!/usr/bin/env bats
# chorus-env-setup.bats — <ROLE>_WERK resolves to the ephemeral werk (#2923).
#
# #2913/#2917 moved werks to the ephemeral chorus-werk/<role>-<card>/ layout
# and deleted the persistent chorus-werk/<role>/ dirs. chorus-env-setup.sh
# still set <ROLE>_WERK to the deleted persistent path. #2923: glob-resolve
# it to the single <role>-<card> werk when exactly one exists; leave it
# unset for zero or many — mirrors resolveWorkingTree + one-card-per-role.

ENV_SETUP="$(cd "$(dirname "${BATS_TEST_FILENAME}")/../scripts" && pwd)/chorus-env-setup.sh"

setup() {
  WERK_BASE=$(mktemp -d)
  export CHORUS_WERK_BASE="$WERK_BASE"
}

teardown() {
  rm -rf "$WERK_BASE"
}

# Source the script with a controlled role + werk base in a subshell, echo
# the resolved <ROLE>_WERK so each test gets a clean env. The werk vars are
# unset first — settings.json statically seeds KADE_WERK/WREN_WERK/SILAS_WERK
# into the session env, so without unsetting them the test would see the
# inherited value, not what chorus-env-setup.sh actually does.
role_werk() {
  local role="$1"
  local var="$(echo "$role" | tr '[:lower:]' '[:upper:]')_WERK"
  ( unset KADE_WERK WREN_WERK SILAS_WERK
    export CHORUS_ROLE="$role"
    source "$ENV_SETUP" >/dev/null 2>&1
    echo "${!var:-}" )
}

@test "exactly one ephemeral werk: <ROLE>_WERK resolves to it" {
  mkdir -p "$WERK_BASE/kade-2923"
  run role_werk kade
  [ "$output" = "$WERK_BASE/kade-2923" ]
}

@test "zero ephemeral werks: <ROLE>_WERK is unset/empty" {
  run role_werk kade
  [ -z "$output" ]
}

@test "multiple ephemeral werks: <ROLE>_WERK is unset/empty (ambiguous)" {
  mkdir -p "$WERK_BASE/kade-2923" "$WERK_BASE/kade-2924"
  run role_werk kade
  [ -z "$output" ]
}

@test "<ROLE>_WERK is never the deleted persistent path chorus-werk/<role>" {
  mkdir -p "$WERK_BASE/kade-2923"
  run role_werk kade
  [ "$output" != "$WERK_BASE/kade" ]
}

@test "only the active role's WERK var is set, not other roles'" {
  mkdir -p "$WERK_BASE/kade-2923" "$WERK_BASE/wren-3000" "$WERK_BASE/silas-3001"
  run bash -c "unset KADE_WERK WREN_WERK SILAS_WERK; export CHORUS_ROLE=kade; source '$ENV_SETUP' >/dev/null 2>&1; echo \"\${WREN_WERK:-unset}/\${SILAS_WERK:-unset}\""
  [ "$output" = "unset/unset" ]
}
