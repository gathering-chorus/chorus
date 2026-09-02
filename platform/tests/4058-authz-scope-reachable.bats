#!/usr/bin/env bats
# @test-type: unit
# 4058 — a surface may not require a scope no principal holds.
#
# WHY THIS EXISTS. From 2026-07-08 the security model declared surfaces requiring
# urn:chorus:domains:code. No principal was ever granted it. Every caller got 403
# out-of-scope on those surfaces for eight weeks and nothing noticed, because a
# surface nobody can reach looks exactly like a surface nobody called. It only
# surfaced when #4015 made per-case jest results storable and 48 x "Received:
# 403" appeared at once. Live count when this was written: 15 unreachable
# surfaces across 6 unheld scopes.
#
# The grant fixes today. This fixes the class. Every assertion is written
# `... || return 1` — a bare [[ ]] does not abort a test here, so an earlier
# failing assertion passes silently (learned the hard way on #4057).

setup() {
  SCRIPT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/scripts/authz-scope-reachable.sh"
  [ -f "$SCRIPT" ] || skip "authz-scope-reachable.sh not found"
  W="$BATS_TEST_TMPDIR"
  export AUTHZ_HELD_FILE="$W/held.txt"
  export AUTHZ_REQUIRED_FILE="$W/required.txt"
}

@test "NEGATIVE PROOF: a surface requiring an UNHELD scope makes the check FAIL" {
  # The exact 2026-07-08 state: the scope is declared by a surface and granted
  # to nobody. This is the fixture the old world had no detector for.
  printf 'urn:chorus:ops\n' > "$AUTHZ_HELD_FILE"
  printf 'urn:chorus:ops\nurn:chorus:domains:code\n' > "$AUTHZ_REQUIRED_FILE"

  run bash "$SCRIPT"
  [ "$status" -eq 1 ] || { echo "expected exit 1, got $status: $output"; return 1; }
  [[ "$output" == *"UNREACHABLE_SURFACES=1"* ]] || { echo "wrong count: $output"; return 1; }
  [[ "$output" == *"urn:chorus:domains:code"* ]] || { echo "scope not named: $output"; return 1; }
}

@test "NEGATIVE PROOF: it counts SURFACES, not scopes — 7 dead surfaces is not '1 problem'" {
  # domains:code alone blocks seven surfaces. A detector that reported "1 unheld
  # scope" would understate the outage sevenfold and read as a nit.
  printf 'urn:chorus:ops\n' > "$AUTHZ_HELD_FILE"
  { for _ in 1 2 3 4 5 6 7; do echo 'urn:chorus:domains:code'; done; } > "$AUTHZ_REQUIRED_FILE"

  run bash "$SCRIPT"
  [[ "$output" == *"UNREACHABLE_SURFACES=7"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"UNREACHABLE_SCOPES=1"* ]] || { echo "$output"; return 1; }
}

@test "every required scope held by someone → PASS" {
  printf 'urn:chorus:ops\nurn:chorus:instances\n' > "$AUTHZ_HELD_FILE"
  printf 'urn:chorus:ops\nurn:chorus:instances\nurn:chorus:ops\n' > "$AUTHZ_REQUIRED_FILE"

  run bash "$SCRIPT"
  [ "$status" -eq 0 ] || { echo "expected pass, got $status: $output"; return 1; }
  [[ "$output" == *"UNREACHABLE_SURFACES=0"* ]] || { echo "$output"; return 1; }
}

@test "NEGATIVE PROOF: an unreadable store reports UNMEASURED, never zero" {
  # The benign default IS the defect. If the store cannot answer, "0 unreachable"
  # is the same clean bill the system gave itself for eight weeks. It must refuse.
  export AUTHZ_HELD_FILE="$W/does-not-exist.txt"
  export AUTHZ_REQUIRED_FILE="$W/also-missing.txt"

  run bash "$SCRIPT"
  [ "$status" -eq 2 ] || { echo "expected exit 2 (unmeasured), got $status: $output"; return 1; }
  [[ "$output" == *"UNMEASURED"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"UNREACHABLE_SURFACES=0"* ]] || { echo "reported a clean bill it could not measure"; return 1; }
}

@test "NEGATIVE PROOF: zero surfaces declaring any scope is UNMEASURED, not clean" {
  # An empty requiresScope set means the model is absent or the query is wrong.
  # Passing on it is how a guard whose target got renamed passes vacuously.
  printf 'urn:chorus:ops\n' > "$AUTHZ_HELD_FILE"
  : > "$AUTHZ_REQUIRED_FILE"

  run bash "$SCRIPT"
  [ "$status" -eq 2 ] || { echo "expected exit 2 (unmeasured), got $status: $output"; return 1; }
  [[ "$output" == *"UNMEASURED"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"UNREACHABLE_SURFACES=0"* ]] || { echo "PASSED VACUOUSLY on an empty model: $output"; return 1; }
}
