#!/usr/bin/env bats
# @test-type: unit — hermetic: sources chorus-env-setup.sh in isolated subshells
# @domain: version-control — the product domain this suite guards (#4334)
# (own mktemp WERK_BASE, forced cwd, unset role vars), no live service touched.
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
# the resolved <ROLE>_WERK so each test gets a clean env. TWO leak vectors are
# neutralized: (1) the werk vars are unset first — settings.json statically
# seeds KADE_WERK/WREN_WERK/SILAS_WERK into the session env; (2) cwd is forced
# to $WERK_BASE (the mktemp fixture, matching no role pattern) because
# chorus-env-setup.sh:44 derives CHORUS_ROLE from $PWD (#3959) and would
# otherwise override the test's exported role when bats runs from a
# chorus-werk/<role>-* cwd (the werk-pipeline case — clean shells matched
# nothing, so this stayed green until a werk run exposed it).
role_werk() {
  local role="$1"
  local var="$(echo "$role" | tr '[:lower:]' '[:upper:]')_WERK"
  ( cd "$WERK_BASE"
    unset KADE_WERK WREN_WERK SILAS_WERK
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
  run bash -c "cd '$WERK_BASE'; unset KADE_WERK WREN_WERK SILAS_WERK; export CHORUS_ROLE=kade; source '$ENV_SETUP' >/dev/null 2>&1; echo \"\${WREN_WERK:-unset}/\${SILAS_WERK:-unset}\""
  [ "$output" = "unset/unset" ]
}

# --- #4515: the old per-role MCP port is gone ---

@test "NEGATIVE PROOF: env-setup exports no CHORUS_MCP_PORT, even with an old werk marker" {
  mkdir -p "$WERK_BASE/silas-3016/.werk-mcp"
  touch "$WERK_BASE/silas-3016/.werk-mcp/active"
  run bash -c "cd '$WERK_BASE'; unset KADE_WERK WREN_WERK SILAS_WERK CHORUS_MCP_PORT CHORUS_MCP_PORT_CANONICAL CHORUS_MCP_WERK_PORT; export CHORUS_ROLE=silas; source '$ENV_SETUP' >/dev/null 2>&1; echo \"[\${CHORUS_MCP_PORT:-}\${CHORUS_MCP_PORT_CANONICAL:-}\${CHORUS_MCP_WERK_PORT:-}]\""
  [ "$output" = "[]" ] || return 1
}

# #4458 — Abby (abby-normal) works cards like the other agents: her session and
# her werk resolve her role and her werk var.
@test "abby-normal: her werk resolves to ABBY_NORMAL_WERK" {
  mkdir -p "$WERK_BASE/abby-normal-4456"
  run bash -c "cd '$WERK_BASE' && unset ABBY_NORMAL_WERK && export CHORUS_ROLE=abby-normal && source '$ENV_SETUP' >/dev/null 2>&1 && echo \"\${ABBY_NORMAL_WERK:-}\""
  [ "$output" = "$WERK_BASE/abby-normal-4456" ] || return 1
}

@test "abby-normal: a shell inside her werk is attributed to her" {
  mkdir -p "$WERK_BASE/chorus-werk/abby-normal-4456"
  run bash -c "cd '$WERK_BASE/chorus-werk/abby-normal-4456' && unset CHORUS_ROLE DEPLOY_ROLE && source '$ENV_SETUP' >/dev/null 2>&1 && echo \"\$CHORUS_ROLE \$DEPLOY_ROLE\""
  [ "$output" = "abby-normal abby-normal" ] || return 1
}

@test "NEGATIVE PROOF: a werk named for a role with no case is not attributed to abby-normal" {
  mkdir -p "$WERK_BASE/chorus-werk/abby-4456"
  run bash -c "cd '$WERK_BASE/chorus-werk/abby-4456' && unset CHORUS_ROLE DEPLOY_ROLE && source '$ENV_SETUP' >/dev/null 2>&1 && echo \"[\${CHORUS_ROLE:-}]\""
  [ "$output" = "[]" ] || return 1
}
