#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# Tests for skill-lifecycle.html — nudge architecture section (#2031)

@test "skill-lifecycle.html exists" {
  [ -f "${CHORUS_ROOT}/docs/diagrams/skill-lifecycle.html" ]
}

@test "skill-lifecycle.html has nudge architecture section" {
  grep -q 'id="nudge"' ${CHORUS_ROOT}/docs/diagrams/skill-lifecycle.html
}

@test "skill-lifecycle.html references chorus-inject delivery chain" {
  grep -q 'chorus-inject' ${CHORUS_ROOT}/docs/diagrams/skill-lifecycle.html
}

@test "skill-lifecycle.html marks nudge as SPOF for 6 skills" {
  grep -q '6 skills' ${CHORUS_ROOT}/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — lifecycle diagram" {
  grep -q 'id="lifecycle"' ${CHORUS_ROOT}/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — delegation map" {
  grep -q 'id="delegation"' ${CHORUS_ROOT}/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — shared infrastructure" {
  grep -q 'id="shared"' ${CHORUS_ROOT}/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — cross-role interactions" {
  grep -q 'id="crossrole"' ${CHORUS_ROOT}/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — decision enforcement" {
  grep -q 'id="decisions"' ${CHORUS_ROOT}/docs/diagrams/skill-lifecycle.html
}
