#!/usr/bin/env bats
# @test-type: contract — the garden-observability repo, read from disk; its own check scripts are the proof
# @domain: gathering — garden-observability, the second product Chorus builds outside itself (#4796)
#
# Chorus builds it, it lives without Chorus: every check runs a script that lives
# in the garden repo, so the repo can prove itself with Chorus stopped.
# A missing repo or a missing check script is a FAIL, never a skip (#3734).

load test_helper

# Before the land, the card's work is on branch wren/4796 in its own garden worktree;
# after Jeff's go merges it, the garden main checkout carries it. Both sit beside chorus.
PARENT="$(dirname "$CHORUS_ROOT")"
if [ "$(basename "$PARENT")" = "chorus-werk" ]; then PROJECTS="$(dirname "$PARENT")"; else PROJECTS="$PARENT"; fi
GARDEN_MAIN="$PROJECTS/garden-observability"
GARDEN_WERK="${CHORUS_WERK_BASE:-$PROJECTS/chorus-werk}/wren-4796-garden"
if [ -z "${GARDEN_REPO:-}" ]; then
  if [ -d "$GARDEN_WERK" ]; then GARDEN_REPO="$GARDEN_WERK"; else GARDEN_REPO="$GARDEN_MAIN"; fi
fi

need_script() {
  [ -x "$GARDEN_REPO/designing/tests/$1" ] || { echo "missing: designing/tests/$1"; return 1; }
}

@test "AC1: the repo exists in the wine-diary layout, pointed at WJeffBridwell/garden-observability" {
  [ -d "$GARDEN_REPO" ] || { echo "no repo at $GARDEN_REPO"; false; }
  for d in designing/ontology designing/data features docs; do
    [ -d "$GARDEN_REPO/$d" ] || { echo "missing dir: $d"; false; }
  done
  run git -c safe.directory='*' -C "$GARDEN_REPO" remote get-url origin
  [ "$status" -eq 0 ]
  [[ "$output" == *"WJeffBridwell/garden-observability"* ]] || { echo "origin is $output"; false; }
}

@test "AC2: the model has where, what, do and see; good.ttl conforms and every bad row is refused" {
  for c in House Floor Room Yard Zone Bed Cell Infra Plant Taxon Animal Practice Observation Season; do
    grep -qE "garden:$c +a +owl:Class" "$GARDEN_REPO"/designing/ontology/*.ttl \
      || { echo "class missing from the model: garden:$c"; false; }
  done
  need_script check-shapes.sh
  run "$GARDEN_REPO/designing/tests/check-shapes.sh"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "AC3: feature 01 exists and names only classes the model has" {
  ls "$GARDEN_REPO"/features/01-*.feature >/dev/null 2>&1 || { echo "no features/01-*.feature"; false; }
  need_script check-feature-classes.sh
  run "$GARDEN_REPO/designing/tests/check-feature-classes.sh"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "AC4: the generated API writes a plant at a cell and reads it back" {
  need_script check-api-roundtrip.sh
  run "$GARDEN_REPO/designing/tests/check-api-roundtrip.sh"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "AC5: the seed loads the plant walk and the 2025 notebook as Season 2025 observations" {
  need_script check-seed.sh
  run "$GARDEN_REPO/designing/tests/check-seed.sh"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "AC6: the phone page serves the season through the garden API, and says so when the API is down" {
  need_script check-page.sh
  run "$GARDEN_REPO/designing/tests/check-page.sh"
  echo "$output"
  [ "$status" -eq 0 ]
}
