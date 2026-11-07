#!/usr/bin/env bats
# @test-type: unit — append_only_ownerless from platform/tests/lib/quartet-walk.sh and scope-plan.py's verdict, over planted manifests
# @domain: tests
# @card: #4915 · owner: kade
# The quartet holds acl:Append on some graphs. Append opens a create only (#4517);
# on a class with no row owner the update, delete and cleanup are refused, so each
# nightly left its probe row in prod (111 rows by 11-06). Silas 11-06: skip the
# create (refuseIfNoCleanup), report NOT-PERM, and the scope verdict takes it.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PLAN="$ROOT/platform/tests/lib/scope-plan.py"

setup() {
  eval "$(sed -n '/^append_only_ownerless() {/,/^}/p' "$ROOT/platform/tests/lib/quartet-walk.sh")"
  declare -F append_only_ownerless >/dev/null || { echo "append_only_ownerless is gone from quartet-walk.sh"; return 1; }
  APPEND_ONLY="urn:chorus:domains:tests urn:chorus:domains:code"
  printf '{"requiredFields":[{"field":"label"}]}' > "$BATS_TEST_TMPDIR/ownerless.json"
  printf '{"requiredFields":[{"field":"label"},{"field":"ownedBy","kind":"edge"}]}' > "$BATS_TEST_TMPDIR/owned.json"
}

@test "an ownerless class on an append-only graph skips the create" {
  append_only_ownerless "$BATS_TEST_TMPDIR/ownerless.json" urn:chorus:domains:tests || return 1
}

@test "negative proof: an owned class under Append still runs the full cycle" {
  ! append_only_ownerless "$BATS_TEST_TMPDIR/owned.json" urn:chorus:domains:tests || return 1
}

@test "negative proof: an ownerless class on a graph the owner may write still runs, and an unknown graph is never skipped" {
  ! append_only_ownerless "$BATS_TEST_TMPDIR/ownerless.json" urn:chorus:domains:board || return 1
  ! append_only_ownerless "$BATS_TEST_TMPDIR/ownerless.json" "" || return 1
}

@test "the scope verdict takes the named skip, and still reds any other refusal of an allowed class" {
  printf 'allow Actor\nallow Language\n' > "$BATS_TEST_TMPDIR/expect"
  printf '%s\n' 'Actor                  NOT-PERM append-only on an ownerless class: a create would leave a row (refuseIfNoCleanup) — urn:chorus:domains:tests' \
                'Language               PASS     create read update delete' > "$BATS_TEST_TMPDIR/out"
  run python3 "$PLAN" verdict "$BATS_TEST_TMPDIR/expect" "$BATS_TEST_TMPDIR/out"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  printf '%s\n' 'Actor                  NOT-PERM { "error": "authz", "message": "write refused" }' \
                'Language               PASS     create read update delete' > "$BATS_TEST_TMPDIR/out"
  run python3 "$PLAN" verdict "$BATS_TEST_TMPDIR/expect" "$BATS_TEST_TMPDIR/out"
  [ "$status" -ne 0 ] || { echo "a plain refusal of an allowed class passed"; return 1; }
  [[ "$output" == *"Actor: rows allow, door answered NOT-PERM"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: when the Permission rows cannot be read the walk refuses, never creates" {
  block="$(sed -n '/^PERMS_JSON="\$(curl/,/^}/p' "$ROOT/platform/tests/lib/quartet-walk.sh")"
  [ -n "$block" ] || { echo "the refusal block is gone from quartet-walk.sh"; return 1; }
  mkdir -p "$BATS_TEST_TMPDIR/bin"; printf '#!/bin/sh\nexit 22\n' > "$BATS_TEST_TMPDIR/bin/curl"; chmod +x "$BATS_TEST_TMPDIR/bin/curl"
  run env PATH="$BATS_TEST_TMPDIR/bin:$PATH" API=http://x bash -c "$block; echo SENTINEL-CONTINUED-4915"
  [ "$status" -eq 3 ] || { echo "$output"; return 1; }
  [[ "$output" != *SENTINEL-CONTINUED-4915* ]] || return 1
}
