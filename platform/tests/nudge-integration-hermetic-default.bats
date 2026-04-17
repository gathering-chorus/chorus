#!/usr/bin/env bats
# nudge-integration-hermetic-default.bats — lock #2165/#2166 hermetic invariant.
#
# Invariant: default `npx jest tests/nudge-integration.test.ts` fires zero
# role.nudge delivered (mode=injected) events. AC #3.1/3.2/3.3 exercise the
# real chorus-hook-shim, but runNudge() sets CHORUS_INJECT_DRY_RUN=1 which
# short-circuits inject_by_tab_name in the shim (#2166).
#
# History: #2149 gated behind HERMETIC_TEST_MODE=describe.skip. #2165 flipped
# polarity to RUN_LIVE_NUDGE opt-in after a 17-nudge storm at 15:46 Boston.
# #2166 removed the describe.skip gate entirely — dry-run at the shim level
# is the hermetic mechanism. Without this lock, a future refactor could drop
# the env var or reintroduce an opt-in gate and the storm returns silently.

TEST_FILE="/Users/jeffbridwell/CascadeProjects/chorus/directing/clearing/tests/nudge-integration.test.ts"

@test "runNudge helper sets CHORUS_INJECT_DRY_RUN so shim skips osascript" {
  grep -qE "CHORUS_INJECT_DRY_RUN:\s*'1'" "$TEST_FILE"
}

@test "nudge-integration does NOT use describe.skip opt-in polarity (#2165 era)" {
  ! grep -qE "RUN_LIVE_NUDGE \? describe : describe\.skip" "$TEST_FILE"
}

@test "nudge-integration does NOT use HERMETIC_TEST_MODE opt-out (pre-#2165)" {
  ! grep -qE "HERMETIC_TEST_MODE \? describe\.skip : describe" "$TEST_FILE"
}

@test "nudge-integration has no describe.skip aliases that run on no env" {
  ! grep -qE "const [a-z] = process\.env\.\w+ \?" "$TEST_FILE"
}
