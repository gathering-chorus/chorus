#!/usr/bin/env bats
# Skill lifecycle diagram verification — #1997

HTML="$BATS_TEST_DIRNAME/skill-lifecycle.html"

@test "HTML file exists" {
  [ -f "$HTML" ]
}

@test "Contains lifecycle flow diagram" {
  grep -q 'Lifecycle' "$HTML"
}

@test "Contains delegation map" {
  grep -q 'Delegation' "$HTML"
}

@test "Contains shared infrastructure diagram" {
  grep -q 'Shared' "$HTML"
}

@test "Contains cross-role interaction map" {
  grep -q 'Cross-Role' "$HTML"
}

@test "Contains decision enforcement audit" {
  grep -q 'Decision' "$HTML"
  grep -q 'DEC-' "$HTML"
}

@test "Shows all lifecycle skills" {
  grep -q 'pull' "$HTML"
  grep -q 'demo' "$HTML"
  grep -q 'acp' "$HTML"
  grep -q 'reboot' "$HTML"
}

@test "Shows gate chain sequence" {
  grep -q 'gate-product' "$HTML"
  grep -q 'gate-code' "$HTML"
  grep -q 'gate-quality' "$HTML"
  grep -q 'gate-arch' "$HTML"
  grep -q 'gate-ops' "$HTML"
}
