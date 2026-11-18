#!/usr/bin/env bats
# @test-type: unit — lib/scope-plan.py's invalid-body probe, on fixtures; nothing touches the door
# @domain: security
# @card: #4517
#
# #4517 AC2 — one source answers "may this principal write this class": the door's decision
# and the Permission rows must agree, both directions, for every role and class. Since AC1 the
# door judges the body only after authz, so ONE fixed, deliberately invalid create body reads
# the decision without writing anything: 403 = refuses, 422 = allows, anything else is
# unmeasured and named (Silas, 2026-11-17 23:34). The live run of the same probe is AC7.
#
# Covers: platform/tests/lib/scope-plan.py

PLAN="$BATS_TEST_DIRNAME/lib/scope-plan.py"

setup() {
  T="$BATS_TEST_TMPDIR"
  printf '{"error":"authz","message":"no Write row for kade"}' > "$T/403"
  printf '{"error":"validation","message":"zz-invalid is not a property"}' > "$T/422"
  printf 'upstream timeout' > "$T/502"
  printf 'deny Credential\nallow CodeFile\nappend Note\n' > "$T/plan"
}

probe() { # class code bodyfile -> one runner line
  python3 "$PLAN" classify "$1" "$2" "$T/$3"
}

@test "AC2 classify: 403 is a refusal, 422 is an allow, anything else is unmeasured and named" {
  [[ "$(probe Credential 403 403)" == "Credential NOT-PERM "* ]] || return 1
  [[ "$(probe CodeFile 422 422)" == "CodeFile PASS door allows"* ]] || return 1
  [[ "$(probe Note 502 502)" == "Note UNMEASURED door answered 502 upstream timeout" ]] || return 1
}

@test "AC2 the door agrees with the rows in both directions: clean" {
  { probe Credential 403 403; probe CodeFile 422 422; probe Note 422 422; } > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" == *"3 classes checked · 0 where"* ]] || return 1
}

@test "AC2 NEGATIVE PROOF: rows deny but the door judged the body (422) — red, named" {
  { probe Credential 422 422; probe CodeFile 422 422; probe Note 422 422; } > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  echo "$output"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Credential: rows deny, door answered PASS"* ]] || return 1
}

@test "AC2 NEGATIVE PROOF: rows allow but the door refuses (403) — red, named" {
  { probe Credential 403 403; probe CodeFile 403 403; probe Note 422 422; } > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  [ "$status" -eq 1 ]
  [[ "$output" == *"CodeFile: rows allow, door answered NOT-PERM"* ]] || return 1
}

@test "AC2 NEGATIVE PROOF: an unmeasured answer is red, never read as agreement" {
  { probe Credential 502 502; probe CodeFile 422 422; probe Note 422 422; } > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Credential: rows deny, door answered UNMEASURED"* ]] || return 1
}

@test "AC2 NEGATIVE PROOF: a planned class the door never answered is red (MISSING), not agreement" {
  { probe CodeFile 422 422; probe Note 422 422; } > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Credential: rows deny, door answered MISSING"* ]] || return 1
}
