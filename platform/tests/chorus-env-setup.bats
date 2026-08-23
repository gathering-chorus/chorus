#!/usr/bin/env bats
# @test-type: unit — hermetic: sources chorus-env-setup.sh in isolated subshells
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

# --- #3016: CHORUS_MCP_PORT — daemon try-before-buy endpoint resolution ---

mcp_port() {
  local role="$1"
  ( cd "$WERK_BASE"
    unset KADE_WERK WREN_WERK SILAS_WERK CHORUS_MCP_PORT CHORUS_MCP_PORT_CANONICAL
    export CHORUS_ROLE="$role"
    source "$ENV_SETUP" >/dev/null 2>&1
    echo "${CHORUS_MCP_PORT:-}" )
}

@test "no active werk: CHORUS_MCP_PORT falls back to canonical 3341" {
  run mcp_port silas
  [ "$output" = "3341" ]
}

@test "active werk but no daemon marker: CHORUS_MCP_PORT stays canonical 3341" {
  mkdir -p "$WERK_BASE/silas-3016"
  run mcp_port silas
  [ "$output" = "3341" ]
}

@test "active werk with daemon marker: CHORUS_MCP_PORT resolves to the role's werk port" {
  mkdir -p "$WERK_BASE/silas-3016/.werk-mcp"
  touch "$WERK_BASE/silas-3016/.werk-mcp/active"
  run mcp_port silas
  [ "$output" = "3351" ]
}

@test "per-role werk ports are deterministic and distinct (silas/kade/wren)" {
  for r in silas kade wren; do
    mkdir -p "$WERK_BASE/$r-1/.werk-mcp"
    touch "$WERK_BASE/$r-1/.werk-mcp/active"
  done
  s=$(mcp_port silas); k=$(mcp_port kade); w=$(mcp_port wren)
  [ "$s" = "3351" ]
  [ "$k" = "3352" ]
  [ "$w" = "3353" ]
  [ "$s" != "$k" ] && [ "$k" != "$w" ] && [ "$s" != "$w" ]
}

@test "canonical port is overridable via CHORUS_MCP_PORT_CANONICAL" {
  run bash -c "cd '$WERK_BASE'; unset KADE_WERK WREN_WERK SILAS_WERK CHORUS_MCP_PORT; export CHORUS_ROLE=silas CHORUS_MCP_PORT_CANONICAL=4000; source '$ENV_SETUP' >/dev/null 2>&1; echo \$CHORUS_MCP_PORT"
  [ "$output" = "4000" ]
}
