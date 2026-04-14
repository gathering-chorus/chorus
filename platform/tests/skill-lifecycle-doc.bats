#!/usr/bin/env bats
# Tests for skill-lifecycle.html — nudge architecture section (#2031)

@test "skill-lifecycle.html exists" {
  [ -f "/Users/jeffbridwell/CascadeProjects/chorus/docs/diagrams/skill-lifecycle.html" ]
}

@test "skill-lifecycle.html has nudge architecture section" {
  grep -q 'id="nudge"' /Users/jeffbridwell/CascadeProjects/chorus/docs/diagrams/skill-lifecycle.html
}

@test "skill-lifecycle.html references chorus-inject delivery chain" {
  grep -q 'chorus-inject' /Users/jeffbridwell/CascadeProjects/chorus/docs/diagrams/skill-lifecycle.html
}

@test "skill-lifecycle.html marks nudge as SPOF for 6 skills" {
  grep -q '6 skills' /Users/jeffbridwell/CascadeProjects/chorus/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — lifecycle diagram" {
  grep -q 'id="lifecycle"' /Users/jeffbridwell/CascadeProjects/chorus/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — delegation map" {
  grep -q 'id="delegation"' /Users/jeffbridwell/CascadeProjects/chorus/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — shared infrastructure" {
  grep -q 'id="shared"' /Users/jeffbridwell/CascadeProjects/chorus/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — cross-role interactions" {
  grep -q 'id="crossrole"' /Users/jeffbridwell/CascadeProjects/chorus/docs/diagrams/skill-lifecycle.html
}

@test "existing sections still present — decision enforcement" {
  grep -q 'id="decisions"' /Users/jeffbridwell/CascadeProjects/chorus/docs/diagrams/skill-lifecycle.html
}
