#!/usr/bin/env bats
# nudge-integration-hermetic-default.bats — lock the polarity from #2165.
#
# Invariant: default `npx jest tests/nudge-integration.test.ts` fires zero
# role.nudge.sent events. AC #3.1/3.2/3.3 blocks that invoke the real
# chorus-hook-shim must be opt-IN (RUN_LIVE_NUDGE=1), not opt-out.
#
# Why: at 2026-04-17 15:46 Boston, default jest storm'd every role's
# terminal with 17 live-injected nudges (chorus.log). Fix flipped polarity
# from HERMETIC_TEST_MODE opt-out to RUN_LIVE_NUDGE opt-in. Without this
# lock, a future refactor could flip it back and the storm returns silently.

TEST_FILE="/Users/jeffbridwell/CascadeProjects/chorus/directing/clearing/tests/nudge-integration.test.ts"

@test "nudge-integration uses RUN_LIVE_NUDGE opt-in polarity" {
  grep -qE "const d = process\.env\.RUN_LIVE_NUDGE \? describe : describe\.skip" "$TEST_FILE"
}

@test "nudge-integration does NOT use opt-out polarity that storms by default" {
  ! grep -qE "process\.env\.HERMETIC_TEST_MODE \? describe\.skip : describe" "$TEST_FILE"
}

@test "nudge-integration header documents the opt-in env var" {
  grep -q "RUN_LIVE_NUDGE" "$TEST_FILE"
}
