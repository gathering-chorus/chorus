#!/usr/bin/env bats
# lint-fragments.sh — fitness linter for CLAUDE.md fragment system (#2150)
#
# 6 rules:
#   R1 PRINCIPLE_DRIFT    same principle name, divergent wording across roles
#   R2 DUPLICATION        parallel fragment Jaccard >= 80% (consolidation candidate)
#   R3 STALE              shared fragment has no refs in activity/briefs/sessions
#                         (tiered: shared-infra 90d, operating-norms 30d, principles never)
#   R4 ASYMMETRY          parallel fragment present for 2+ roles but missing for 1
#   R5 DANGLING_DEC       DEC-### citation not in decisions.md
#   R6 SIZE_VARIANCE      parallel fragment line-count variance >50% (warning)
#
# Exit codes: 0 clean, 1 warnings only, 2 errors.

LINTER="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/lint-fragments.sh"

setup() {
  FIXTURE="$(mktemp -d)"
  mkdir -p "$FIXTURE/shared" "$FIXTURE/roles/wren" "$FIXTURE/roles/silas" "$FIXTURE/roles/kade"
  cat > "$FIXTURE/manifest.json" <<EOF
{
  "version": "1",
  "roles": {
    "wren": {"output": "../../roles/wren/CLAUDE.md", "sections": []},
    "silas": {"output": "../../roles/silas/CLAUDE.md", "sections": []},
    "kade": {"output": "../../roles/kade/CLAUDE.md", "sections": []}
  }
}
EOF
}

teardown() {
  rm -rf "$FIXTURE"
}

@test "linter script exists and is executable" {
  [ -x "$LINTER" ]
}

@test "R4 asymmetry: flags fragment present for 2 of 3 roles" {
  printf "## title\nfoo\n" > "$FIXTURE/roles/wren/working-with-jeff.md"
  printf "## title\nbar\n" > "$FIXTURE/roles/silas/working-with-jeff.md"
  # kade is missing
  run "$LINTER" --fixture "$FIXTURE"
  [[ "$output" == *"R4"* ]] || { echo "$output"; false; }
  [[ "$output" == *"working-with-jeff"* ]] || { echo "$output"; false; }
  [[ "$output" == *"kade"* ]] || { echo "$output"; false; }
  [ "$status" -eq 2 ]
}

@test "R4 asymmetry: clean when all three roles have same parallel fragment" {
  for role in wren silas kade; do
    printf "## tone\nbe direct\n" > "$FIXTURE/roles/$role/tone.md"
  done
  run "$LINTER" --fixture "$FIXTURE"
  [[ "$output" != *"R4"* ]] || { echo "$output"; false; }
}

@test "R2 duplication: flags >80% Jaccard similarity across parallel fragments" {
  # Nearly identical tone.md in all three roles
  for role in wren silas kade; do
    cat > "$FIXTURE/roles/$role/tone.md" <<EOF
## Tone

Tone is a practice not a setting. How we communicate reflects how we think
and how we treat each other. Improve it daily with willingness.

- Have a position.
- Be direct, honest feedback is care.
- Stay curious, ask why before forming opinions.
EOF
  done
  run "$LINTER" --fixture "$FIXTURE"
  [[ "$output" == *"R2"* ]] || { echo "$output"; false; }
  [[ "$output" == *"tone"* ]] || { echo "$output"; false; }
}

@test "R2 duplication: strips markdown before tokenizing (Silas's adjustment)" {
  # Same tokens, different markdown structure — must still flag
  printf -- "- stop on error\n- check logs\n- fix root cause\n" > "$FIXTURE/roles/wren/tone.md"
  printf "Stop on error. Check logs. Fix root cause.\n" > "$FIXTURE/roles/silas/tone.md"
  printf "# stop on error\n# check logs\n# fix root cause\n" > "$FIXTURE/roles/kade/tone.md"
  run "$LINTER" --fixture "$FIXTURE"
  [[ "$output" == *"R2"* ]] || { echo "$output"; false; }
}

@test "R2 duplication: does NOT flag role-specific fragments with different content" {
  printf "## Wren portfolio\nGathering + Chorus\n" > "$FIXTURE/roles/wren/portfolio.md"
  printf "## Silas portfolio\nSix projects, see infra-constraints\n" > "$FIXTURE/roles/silas/portfolio.md"
  printf "## Kade portfolio\nPrimary Gathering Express TS\n" > "$FIXTURE/roles/kade/portfolio.md"
  run "$LINTER" --fixture "$FIXTURE"
  [[ "$output" != *"R2 "* ]] || { echo "$output"; false; }
}

@test "R5 dangling DEC: flags DEC-### not in decisions.md" {
  DECISIONS="$FIXTURE/decisions.md"
  printf "## decisions\n- DEC-042 real thing\n" > "$DECISIONS"
  printf "Refers to DEC-99999 which does not exist.\n" > "$FIXTURE/roles/wren/principles.md"
  run "$LINTER" --fixture "$FIXTURE" --decisions "$DECISIONS"
  [[ "$output" == *"R5"* ]] || { echo "$output"; false; }
  [[ "$output" == *"DEC-99999"* ]] || { echo "$output"; false; }
}

@test "R6 size variance: warns when parallel fragment line-counts differ >50%" {
  printf "x\n" > "$FIXTURE/roles/wren/how-you-operate.md"
  # 10 lines for silas
  yes x | head -10 > "$FIXTURE/roles/silas/how-you-operate.md"
  yes x | head -10 > "$FIXTURE/roles/kade/how-you-operate.md"
  run "$LINTER" --fixture "$FIXTURE"
  [[ "$output" == *"R6"* ]] || { echo "$output"; false; }
  # R6 is a warning, not an error — exit 1 not 2 (unless other rules fired)
}

@test "exit code 0 when fixture is fully clean" {
  # three roles all have same well-formed parallel fragments, no duplication, no dangling
  for role in wren silas kade; do
    printf "## Role %s\nspecific content for %s role\n" "$role" "$role" > "$FIXTURE/roles/$role/portfolio.md"
    printf "## working with jeff\n- style tip for %s\n" "$role" > "$FIXTURE/roles/$role/working-with-jeff.md"
  done
  # no decisions file needed if no DEC citations
  run "$LINTER" --fixture "$FIXTURE"
  [ "$status" -eq 0 ]
}

@test "exits 2 on asymmetry (hard error)" {
  printf "x\n" > "$FIXTURE/roles/wren/uniq.md"
  printf "x\n" > "$FIXTURE/roles/silas/uniq.md"
  run "$LINTER" --fixture "$FIXTURE"
  [ "$status" -eq 2 ]
}

@test "linter accepts --json for machine output" {
  printf "x\n" > "$FIXTURE/roles/wren/uniq.md"
  printf "x\n" > "$FIXTURE/roles/silas/uniq.md"
  run "$LINTER" --fixture "$FIXTURE" --json
  [[ "$output" == *"\"rule\""* ]] || { echo "$output"; false; }
  [[ "$output" == *"R4"* ]] || { echo "$output"; false; }
}
