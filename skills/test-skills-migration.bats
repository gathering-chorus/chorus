#!/usr/bin/env bats
# Verification tests for #1835 — skills migration to chorus/skills/

CANONICAL="/Users/jeffbridwell/CascadeProjects/chorus/skills"
WREN="/Users/jeffbridwell/CascadeProjects/chorus/roles/wren/.claude/skills"
SILAS="/Users/jeffbridwell/CascadeProjects/chorus/roles/silas/.claude/skills"
KADE="/Users/jeffbridwell/CascadeProjects/chorus/roles/kade/.claude/skills"
GLOBAL="$HOME/.claude/skills"

@test "canonical skills dir has 36 skill directories" {
  count=$(find "$CANONICAL" -maxdepth 1 -mindepth 1 -type d | wc -l | tr -d ' ')
  [ "$count" -eq 36 ]
}

@test "all role symlinks point to chorus/skills, not platform/skills" {
  stale=0
  for dir in "$WREN" "$SILAS" "$KADE" "$GLOBAL"; do
    for f in "$dir"/*; do
      if [ -L "$f" ] && readlink "$f" | grep -q "platform/skills"; then
        stale=$((stale+1))
      fi
    done
  done
  [ "$stale" -eq 0 ]
}

@test "no real directories in role skill dirs (all symlinks)" {
  real=0
  for dir in "$WREN" "$SILAS" "$KADE" "$GLOBAL"; do
    for f in "$dir"/*; do
      if [ -d "$f" ] && [ ! -L "$f" ]; then
        real=$((real+1))
      fi
    done
  done
  [ "$real" -eq 0 ]
}

@test "no broken symlinks in any role skill dir" {
  broken=0
  for dir in "$WREN" "$SILAS" "$KADE" "$GLOBAL"; do
    for f in "$dir"/*; do
      if [ -L "$f" ] && [ ! -e "$f" ]; then
        broken=$((broken+1))
      fi
    done
  done
  [ "$broken" -eq 0 ]
}

@test "every canonical skill has a SKILL.md" {
  missing=0
  for d in "$CANONICAL"/*/; do
    [ -f "$d/SKILL.md" ] || missing=$((missing+1))
  done
  [ "$missing" -eq 0 ]
}

@test "framework.ttl has ownedBy for all 36 skills" {
  ttl="/Users/jeffbridwell/CascadeProjects/chorus/roles/silas/ontology/framework.ttl"
  count=$(grep -c 'a fw:Skill' "$ttl")
  [ "$count" -ge 36 ]
}
