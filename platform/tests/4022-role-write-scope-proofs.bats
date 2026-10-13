#!/usr/bin/env bats
# @test-type: unit
# @domain: security — the product domain this suite guards (#4334)
# #4022 — proofs for lib/scope-plan.py, the live write-scope check that replaces
# the three per-role quartet walks. Fixtures only; nothing touches the door.
# The check goes red when the door and the Permission rows disagree, in either
# direction (Wren, 2026-10-13).

PLAN="$BATS_TEST_DIRNAME/lib/scope-plan.py"

setup() {
  T="$BATS_TEST_TMPDIR"
  cat > "$T/perms.json" <<'EOF'
{"data":[
 {"agent":"principal-kade","mode":"http://www.w3.org/ns/auth/acl#Write","accessTo":"urn:g:code"},
 {"agent":"principal-kade","mode":"http://www.w3.org/ns/auth/acl#Read","accessTo":"urn:g:security"},
 {"agent":"principal-wren","mode":"http://www.w3.org/ns/auth/acl#Write","accessTo":"urn:g:security"}
]}
EOF
  printf '{"CodeFile":"urn:g:code","Language":"urn:g:code","Credential":"urn:g:security","Policy":"urn:g:security"}\n' > "$T/graphs.json"
}

@test "plan: every class the rows refuse is denied, plus one allowed class" {
  run python3 "$PLAN" plan "$T/perms.json" "$T/graphs.json" kade
  [ "$status" -eq 0 ]
  [ "$output" = "deny Credential
deny Policy
allow CodeFile" ]
}

@test "plan: a Read row is not a write grant" {
  run python3 "$PLAN" plan "$T/perms.json" "$T/graphs.json" kade
  [[ "$output" == *"deny Credential"* ]] || return 1
}

@test "verdict: the door agrees with the rows — clean" {
  printf 'deny Credential\ndeny Policy\nallow CodeFile\n' > "$T/plan"
  printf 'Credential             NOT-PERM {"error":"forbidden"}\nPolicy                 NOT-PERM {"error":"forbidden"}\nCodeFile               PASS     create read update delete\n' > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  [ "$status" -eq 0 ]
  [[ "$output" == *"3 classes checked · 0 where"* ]] || return 1
}

# NEGATIVE PROOF: the rows deny the role, the door lets the write through.
@test "verdict: rows deny but the door writes — red, names the class" {
  printf 'deny Credential\nallow CodeFile\n' > "$T/plan"
  printf 'Credential             PASS     create read update delete\nCodeFile               PASS     ok\n' > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Credential: rows deny, door answered PASS"* ]] || return 1
}

# NEGATIVE PROOF, the other direction: the rows allow, the door refuses.
@test "verdict: rows allow but the door refuses — red, names the class" {
  printf 'deny Credential\nallow CodeFile\n' > "$T/plan"
  printf 'Credential             NOT-PERM {"error":"forbidden"}\nCodeFile               NOT-PERM {"error":"forbidden"}\n' > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  [ "$status" -eq 1 ]
  [[ "$output" == *"CodeFile: rows allow, door answered NOT-PERM"* ]] || return 1
}

@test "verdict: a class the runner never reported is red, not skipped" {
  printf 'deny Credential\n' > "$T/plan"
  : > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  [ "$status" -eq 1 ]
  [[ "$output" == *"Credential: rows deny, door answered MISSING"* ]] || return 1
}

@test "verdict: a door-managed class is not a scope disagreement" {
  printf 'deny Version\n' > "$T/plan"
  printf 'Version                NOT-PERM door-managed class, not writable by any principal: x\n' > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  [ "$status" -eq 0 ]
}

# A class whose graph the door did not report is unmeasured — red, never a deny.
@test "plan + verdict: an unreadable class graph is red, not counted as a refusal" {
  printf '{"CodeFile":"urn:g:code","TestResult":""}\n' > "$T/g2.json"
  run python3 "$PLAN" plan "$T/perms.json" "$T/g2.json" kade
  [[ "$output" == *"unknown TestResult"* ]] || return 1
  python3 "$PLAN" plan "$T/perms.json" "$T/g2.json" kade > "$T/plan"
  printf 'CodeFile               PASS     ok\n' > "$T/out"
  run python3 "$PLAN" verdict "$T/plan" "$T/out"
  [ "$status" -eq 1 ]
  [[ "$output" == *"TestResult: the door did not say which graph"* ]] || return 1
}
