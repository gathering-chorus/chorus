#!/usr/bin/env bats
# @test-type: unit — reads source and model files only; no live services
# @domain: security
# @card: #4517
#
# #4517 AC5/AC6 — the switch-on guard (Silas, 2026-11-18 00:11). Once a Session shape carries
# writeAuthority "door", the door refuses every session write from a role, including logout's
# PATCH endedAt and the sweep. So the shape may carry "door" only in the same land that removes
# every role-side Session write. This fails if the shape says "door" while any remain.
#
# Role-side Session writes, measured 2026-11-18 00:15:
#   chorus-principal lib.rs  do_login POST, patch_changed / patch_row on "identity/sessions"
#   person_session.py        POST / PUT / PATCH on {api}/v1/identity/sessions (Jeff's browser session)

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SHAPES="$(grep -rlE 'SessionShape' "$ROOT/roles" "$ROOT/designing" 2>/dev/null | grep '\.ttl$' || true)"
}

# writes "<file>:<line>" for every role-side Session write site under $1 (a repo root)
role_session_writes() {
  local r="$1"
  grep -nE 'format!\("\{\}/v1/identity/sessions", ctx\.api\)|patch_(changed|row)\([^)]*"identity/sessions"' \
    "$r/platform/services/chorus-principal/src/"*.rs 2>/dev/null
  grep -nE 'http\("(POST|PUT|PATCH|DELETE)", f"\{api\}/v1/identity/sessions' \
    "$r/platform/scripts/lib/person_session.py" 2>/dev/null
  return 0
}

shape_says_door() { # $@ shape files
  [ "$#" -gt 0 ] && grep -qE 'writeAuthority[[:space:]]+"door"' "$@"
}

guard() { # $@ shape files — 1 (and the sites) when a shape says door while role writes remain
  shape_says_door "$@" || return 0
  local w; w="$(role_session_writes "$ROOT")"
  [ -z "$w" ] && return 0
  echo "$w"; return 1
}

@test "AC5 guard: the finder sees today's role-side Session writes (it can go red)" {
  [ -n "$SHAPES" ] || { echo "no Session shape file found — the guard has nothing to read"; return 1; }
  n="$(role_session_writes "$ROOT" | wc -l | tr -d ' ')"
  echo "role-side Session write sites: $n"
  [ "$n" -ge 5 ] || return 1
}

@test "AC5 guard: a Session shape says writeAuthority door only once no role writes Session rows" {
  # shellcheck disable=SC2086
  run guard $SHAPES
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC5 guard NEGATIVE PROOF: a shape flipped to door while role writes remain is red" {
  T="$BATS_TEST_TMPDIR"
  printf 'chorus:SessionShape chorus:writeAuthority "door" .\n' > "$T/shape.ttl"
  run guard "$T/shape.ttl"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"identity/sessions"* ]] || return 1
}
