#!/usr/bin/env bats
# @test-type: unit
# #3598 — Retirement gate contract. Deleting a surface (script/gate/hook/source)
# must delete-or-repoint its referencing tests in the SAME change. The gate
# scans for test files that still reference a just-deleted file; if found, it
# blocks. This is the structural fix for the rot that fed the nightly false-reds
# (git-queue.sh/show-gate.sh/done-gate.sh deleted, their tests left behind).

setup() {
  GATE="$BATS_TEST_DIRNAME/../retirement-gate.sh"
  [ -f "$GATE" ] || skip "retirement-gate.sh not present"
  REPO="$(mktemp -d "${TMPDIR:-/tmp}/retgate-XXXXXX")"
  mkdir -p "$REPO/platform/scripts/tests" "$REPO/platform/scripts"
}

teardown() {
  [ -n "${REPO:-}" ] && rm -rf "$REPO"
}

@test "blocks: a test still references a deleted script" {
  # a retired surface + a test that still greps it
  printf '#!/usr/bin/env bats\n@test "x" { grep -q foo "$BATS_TEST_DIRNAME/../widget.sh"; }\n' \
    > "$REPO/platform/scripts/tests/widget.bats"
  # widget.sh does NOT exist (deleted) and is passed as the deletion
  run env CHORUS_ROOT="$REPO" RETGATE_DELETED="platform/scripts/widget.sh" bash "$GATE"
  [ "$status" -ne 0 ]
  [[ "$output" == *"widget.bats"* ]]
  [[ "$output" == *"widget.sh"* ]]
}

@test "passes: deleting a file no test references" {
  printf '#!/usr/bin/env bats\n@test "x" { true; }\n' \
    > "$REPO/platform/scripts/tests/unrelated.bats"
  run env CHORUS_ROOT="$REPO" RETGATE_DELETED="platform/scripts/orphan-nobody-refs.sh" bash "$GATE"
  [ "$status" -eq 0 ]
}

@test "passes: the referencing test was deleted in the same change (repointed)" {
  # no test file references widget.sh (its test was removed alongside) → clean
  printf '#!/usr/bin/env bats\n@test "x" { true; }\n' \
    > "$REPO/platform/scripts/tests/kept.bats"
  run env CHORUS_ROOT="$REPO" RETGATE_DELETED="platform/scripts/widget.sh" bash "$GATE"
  [ "$status" -eq 0 ]
}

@test "ignores deletion of non-surface files (docs, data)" {
  printf 'something widget.json something\n' > "$REPO/platform/scripts/tests/data.bats"
  run env CHORUS_ROOT="$REPO" RETGATE_DELETED="some/data/widget.json" bash "$GATE"
  [ "$status" -eq 0 ]
}
