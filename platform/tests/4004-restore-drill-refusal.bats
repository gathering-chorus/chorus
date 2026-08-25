#!/usr/bin/env bats
# @test-type: unit — drives the wrapper's exit-code translation with a stub drill
#
# #4004 — restore-drill.sh exits 2 when it declines to measure (bedroom
# unreachable, no backup, no scratch space, load > 12). The nightly scored that
# as a plain failure, so on 2026-08-25 the restore drill went red purely because
# the box was at load 13.2 — an alarm about backups that was really an alarm
# about CPU. rc=3 is the nightly's SELF-REFUSED verdict; that is what an
# unmeasurable drill is. A genuine restore failure must still go red.

setup() {
  R="$BATS_TEST_TMPDIR/repo"
  mkdir -p "$R/platform/scripts"
  cp "$BATS_TEST_DIRNAME/../scripts/test-restore-drill.sh" "$R/platform/scripts/"
  # the wrapper's cadence logic short-circuits on a recent PASS; force the run
  export CHORUS_ROOT="$R" RESTORE_DRILL_MAX_AGE_DAYS=0
}

stub_drill() { printf '#!/usr/bin/env bash\nexit %s\n' "$1" > "$R/platform/scripts/restore-drill.sh"; }

@test "NEGATIVE PROOF: an UNMEASURABLE drill (exit 2) becomes rc=3, not a failure" {
  stub_drill 2
  run bash "$R/platform/scripts/test-restore-drill.sh"
  [ "$status" -eq 3 ]
  [[ "$output" == *"UNMEASURABLE"* ]]
}

@test "NEGATIVE PROOF: a REAL restore failure (exit 1) still goes red" {
  stub_drill 1
  run bash "$R/platform/scripts/test-restore-drill.sh"
  [ "$status" -eq 1 ]
  [[ "$output" != *"SELF-REFUSED"* ]]
}

@test "a proven restore (exit 0) stays green" {
  stub_drill 0
  run bash "$R/platform/scripts/test-restore-drill.sh"
  [ "$status" -eq 0 ]
}
