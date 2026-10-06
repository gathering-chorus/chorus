#!/usr/bin/env bats
# @test-type: integration — drives the built chorus-principal with the login harness's stubs; no live services
# @domain: identity
#
# #4432 — chorus-principal reads the agent roles from the roles door
# (/v1/roles/roles, roleKind agent), not from a list of three. Abby Normal is
# the fourth. When the door does not answer, the command refuses and says so;
# it never falls back to wren | silas | kade.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="${CHORUS_PRINCIPAL_TEST_BIN:-$ROOT/platform/services/chorus-principal/target/release/chorus-principal}"
  [ -x "$SCRIPT" ] || skip "chorus-principal not built at $SCRIPT"
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
}

@test "abby-normal is a role the command accepts" {
  run bash -c "echo '{\"session_id\":\"c-4432\",\"prompt\":\"hi\"}' | AWAKE_SEEN_SYNC=1 '$SCRIPT' seen abby-normal"
  printf '%s' "$output" | grep -vqF "unknown role"
}

@test "a name the door does not list is refused, naming the door's roles" {
  run "$SCRIPT" relogin nobody
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "unknown role 'nobody' (abby-normal | kade | silas | wren)"
}

@test "NEGATIVE PROOF: with the door down even wren is refused, loudly" {
  touch "$T/roles-door-down"
  run bash -c "echo '{\"session_id\":\"c-4432\",\"prompt\":\"hi\"}' | AWAKE_SEEN_SYNC=1 '$SCRIPT' seen wren"
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "REFUSED"
}

# #4432 — the places Abby has to show up read the role list; none types three names.
typed_lists() {
  grep -nE "'(wren|silas|kade)', *'(wren|silas|kade)', *'(wren|silas|kade)'|\(wren\|silas\|kade\)" \
    "$1/directing/clearing/src/tiles.ts" "$1/directing/clearing/src/tiles-spine.ts" "$1/directing/clearing/src/router.ts" \
    "$1/directing/clearing/src/server.ts" "$1/directing/clearing/src/spine-tail.ts" "$1/directing/clearing/src/chat.ts" \
    "$1/directing/clearing/public/index.html" "$1/directing/clearing/public/clearing-tree.js" \
    "$1/platform/api/src/handlers/context-roles.ts" "$1/platform/api/public/chorus-pages/loom.html" \
    "$1/platform/api/public/chorus-pages/werk.html" "$1/platform/api/views/team.ejs" "$1/platform/pulse/src/store.ts" \
    "$1/platform/mcp-server/src/main-stdio.ts" "$1/platform/services/chorus-hooks/src/hooks/nudge_drain.rs" 2>/dev/null
}

@test "no tile, mention, filter or nudge path types the three role names" {
  run typed_lists "$ROOT"
  echo "$output"
  test -z "$output"
}

@test "NEGATIVE PROOF: the same check finds the typed lists main still has" {
  M="$BATS_TEST_TMPDIR/main"; mkdir -p "$M/directing/clearing/src"
  git -C "$ROOT" show origin/main:directing/clearing/src/tiles.ts > "$M/directing/clearing/src/tiles.ts"
  run typed_lists "$M"
  test -n "$output"
}
