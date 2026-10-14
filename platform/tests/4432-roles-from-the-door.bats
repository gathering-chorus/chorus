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

# Driven through relogin, a verb that reads the role list. `seen` is the
# every-turn hook and takes any principal name without asking the door
# (#4328: never refused, never slow), so it cannot show the refusal; driving
# it here exited 0 and reddened main from 10-14.
@test "NEGATIVE PROOF: with the door down even wren is refused, loudly" {
  touch "$T/roles-door-down"
  run "$SCRIPT" relogin wren
  test "$status" -eq 2
  printf '%s' "$output" | grep -qF "REFUSED"
}

# #4432 — the places Abby has to show up read the role list; none types three names.
# #4515 — the old MCP server's stdio entry left the list with that server.
SCANNED=(
  directing/clearing/src/tiles.ts directing/clearing/src/tiles-spine.ts directing/clearing/src/router.ts
  directing/clearing/src/server.ts directing/clearing/src/spine-tail.ts directing/clearing/src/chat.ts
  directing/clearing/public/index.html directing/clearing/public/clearing-tree.js
  platform/api/src/handlers/context-roles.ts platform/api/public/chorus-pages/loom.html
  platform/api/public/chorus-pages/werk.html platform/api/views/team.ejs platform/pulse/src/store.ts
  platform/services/chorus-hooks/src/hooks/nudge_drain.rs
  platform/services/chorus-hooks/src/runtime_hook.rs
  platform/services/chorus-agent/src/server.rs
)

typed_lists() {
  local f files=()
  for f in "${SCANNED[@]}"; do files+=("$1/$f"); done
  grep -nE "'(wren|silas|kade)', *'(wren|silas|kade)', *'(wren|silas|kade)'|\(wren\|silas\|kade\)|\"(wren|silas|kade)\" *\| *\"(wren|silas|kade)\" *\| *\"(wren|silas|kade)\"" \
    "${files[@]}" 2>/dev/null
}

# A scanned file that moved or was deleted would pass the scan by being absent.
missing_targets() {
  local f
  for f in "${SCANNED[@]}"; do [ -f "$1/$f" ] || echo "MISSING $f"; done
}

@test "every file the typed-list scan reads exists (a gone target fails loud)" {
  run missing_targets "$ROOT"
  echo "$output"
  test -z "$output"
}

@test "NEGATIVE PROOF: a scanned file that is gone is named" {
  run missing_targets "$BATS_TEST_TMPDIR/empty-tree"
  echo "$output"
  printf '%s' "$output" | grep -qF 'MISSING platform/pulse/src/store.ts'
}

@test "no tile, mention, filter or nudge path types the three role names" {
  run typed_lists "$ROOT"
  echo "$output"
  test -z "$output"
}

# The proofs below plant the old code as fixed text. They used to read it from
# origin/main, which stopped having it the moment #4432 landed, so the proof
# went red on main (Kade, 10-06 16:18).
@test "NEGATIVE PROOF: the check finds a runtime hook that enrolls three typed names" {
  M="$BATS_TEST_TMPDIR/old-hook"; mkdir -p "$M/platform/services/chorus-hooks/src"
  printf '%s\n' '    if !matches!(role.as_str(), "wren" | "silas" | "kade") { return Err("not enrolled".into()); }' \
    > "$M/platform/services/chorus-hooks/src/runtime_hook.rs"
  run typed_lists "$M"
  echo "$output"
  printf '%s' "$output" | grep -qF 'runtime_hook.rs'
}

@test "NEGATIVE PROOF: the same check finds tiles built from a typed list" {
  M="$BATS_TEST_TMPDIR/old-tiles"; mkdir -p "$M/directing/clearing/src"
  printf '%s\n' "const ROLES = ['wren', 'silas', 'kade'] as const;" > "$M/directing/clearing/src/tiles.ts"
  run typed_lists "$M"
  echo "$output"
  printf '%s' "$output" | grep -qF 'tiles.ts'
}
