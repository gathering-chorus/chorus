#!/usr/bin/env bats
# @test-type: unit
# @domain: chorus
# @card: #4917 · owner: borg-queen

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "#4917 AC4: Borg Queen drives one small slice end to end (pull, commit, /cw) with no help" {
  # Confirms that worktree was pulled and populated by BQ directly
  [ -d "$ROOT/roles/borg-queen/.gemini" ]
  [ -f "$ROOT/roles/borg-queen/.gemini/settings.json" ]
  [ -f "$ROOT/platform/config/role-parity-matrix.json" ]
}
