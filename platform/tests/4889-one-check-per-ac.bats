#!/usr/bin/env bats
# @test-type: unit — the pipelines' proof layers counted from the generated workflows; the AC check rule on fixtures
# @domain: pipelines
# @card: #4889 · owner: silas
# What Jeff sees: a card goes through fewer checks, each one means something.
# Jeff 2026-11-04 17:48: "now we have like 10 overlapping vouches and proves and
# local proofs and werk test". The 30-day inventory is
# designing/docs/proof-layers-4889.md; this suite holds the cut to it.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PIPES="$ROOT/platform/pipelines"
INV="$ROOT/designing/docs/proof-layers-4889.md"
DEMO_SRC="$ROOT/platform/services/werk-demo/src/lib.rs"

# The proof layers a card passes through, read from what actually runs: the
# generated workflows and werk-demo's vouch rule. One line per layer present.
layers() { # PIPES_DIR DEMO_SRC
  local p="$1" d="$2"
  grep -q 'name: skill-pair-green' "$p/pair.yaml" && echo "pair green"
  grep -q 'pipeline-start handoff' "$p/pair.yaml" && echo "pair local proof"
  grep -q 'name: skill-werk-test-pipeline' "$p/cicd.yaml" && echo "cicd werk-test"
  grep -q 'name: skill-werk-review$' "$p/cicd.yaml" && echo "review floor"
  grep -q 'name: skill-werk-demo-gate-' "$p/demo.yaml" && echo "gates"
  grep -q 'name: skill-werk-demo-review-' "$p/demo.yaml" && echo "peer reviews"
  grep -q 'name: skill-werk-demo-session-smoke' "$p/demo.yaml" && echo "demo smoke"
  grep -q 'vouch_of(witness, card, round, patch, "navigator"' "$d" && echo "navigator vouch"
  grep -q '"owner"' "$d" && echo "owner vouch"
  grep -q 'name: skill-werk-go' "$p/cicd-go.yaml" && echo "jeff go"
  grep -q 'name: skill-borg-verify-in-prod' "$p/cicd-land.yaml" && echo "prod verify"
  return 0
}

# The ones #4889 retires (inventory, Kade 18:00/18:03); none may run after the land.
RETIRED="gates|navigator vouch|demo smoke"

@test "AC1: the 30-day inventory names every layer with its owner, runs and what it caught alone" {
  [ -f "$INV" ] || { echo "no inventory at $INV"; return 1; }
  rows="$(grep -cE '^\| [0-9]+ \| ' "$INV")"
  [ "$rows" -ge 11 ] || { echo "only $rows layer rows"; return 1; }
  grep -q 'Alone, real defect' "$INV" || return 1
  grep -q '^## Retire' "$INV" || return 1
}

@test "AC3: the layer count goes down — fewer than 11 run, and none of the retired ones" {
  out="$(layers "$PIPES" "$DEMO_SRC")"
  echo "$out"
  n="$(printf '%s\n' "$out" | grep -c .)"
  [ "$n" -lt 11 ] || { echo "$n layers"; return 1; }
  ! printf '%s\n' "$out" | grep -qxE "$RETIRED" || { echo "a retired layer still runs"; return 1; }
}

@test "AC3 negative proof: today's main, with every layer, counts 11 or more and fails" {
  d="$BATS_TEST_TMPDIR/p"; mkdir -p "$d"
  git -C "$ROOT" show origin/main:platform/pipelines/pair.yaml > "$d/pair.yaml"
  for f in cicd demo cicd-go cicd-land; do git -C "$ROOT" show "origin/main:platform/pipelines/$f.yaml" > "$d/$f.yaml"; done
  git -C "$ROOT" show origin/main:platform/services/werk-demo/src/lib.rs > "$d/lib.rs"
  n="$(layers "$d" "$d/lib.rs" | grep -c .)"
  [ "$n" -ge 11 ] || { echo "main counts only $n — the counter cannot see the layers"; return 1; }
}

@test "AC4: no new layer — every layer that runs is one the inventory keeps" {
  keep="pair green|pair local proof|cicd werk-test|review floor|peer reviews|owner vouch|jeff go|prod verify"
  out="$(layers "$PIPES" "$DEMO_SRC")"
  bad="$(printf '%s\n' "$out" | grep -vxE "$keep" || true)"
  [ -z "$bad" ] || { echo "not in the kept list: $bad"; return 1; }
}

# AC2 / AC5 — every AC names exactly one check, refused at the demo's validate
# step (the one place left before Jeff's go), not only at pull: #4888's checks were
# lost by an AC rewrite after the pull and every run read "unproven".
validate() { # DESC_FILE
  BORG_CARD_DESC="$1" bash -c "cd '$ROOT' && werk-demo validate 4889 silas" 2>&1
}

@test "AC2: a card whose every AC names one check passes validate" {
  printf '## AC\n- [x] One. check: test platform/tests/4889-one-check-per-ac.bats\n- [x] Two. check: http /api/chorus/health 200\n' > "$BATS_TEST_TMPDIR/ok.md"
  run validate "$BATS_TEST_TMPDIR/ok.md"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC5 negative proof: an AC with no check fails validate, and names it" {
  printf '## AC\n- [x] One. check: test platform/tests/4889-one-check-per-ac.bats\n- [x] Two has no check.\n' > "$BATS_TEST_TMPDIR/nocheck.md"
  run validate "$BATS_TEST_TMPDIR/nocheck.md"
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"Two has no check"* ]] || { echo "$output"; return 1; }
}

@test "AC5 negative proof: a check that names a test file not in the repo fails validate" {
  printf '## AC\n- [x] One. check: test platform/tests/no-such-suite-4889.bats\n' > "$BATS_TEST_TMPDIR/ghost.md"
  run validate "$BATS_TEST_TMPDIR/ghost.md"
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"no-such-suite-4889.bats"* ]] || { echo "$output"; return 1; }
}
