#!/usr/bin/env bats
# @test-type: unit — retirement guard
load test_helper
# nudge-inject-ack.bats — RETIREMENT GUARD (#3710)
#
# #2036 fixed a real and nasty bug: the bash `nudge` printed "DELIVERED" even
# when inject failed, so the Clearing read stdout and told Jeff "Sent" for a
# nudge that never arrived. These tests guarded that.
#
# Both surfaces they poked are gone: the bash `nudge` script was retired by
# #2804 and deleted by #2809 (agents nudge via the chorus_nudge_message MCP
# tool), and chorus-hooks/src/nudge.rs went with it. So the cases were shelling
# out to a missing script (exit 127) and grepping a missing file.
#
# THE INVARIANT SURVIVED THE IMPLEMENTATION, so this is not a plain deletion.
# "A failed delivery is never reported as delivered" now lives in the pulse
# delivery worker and is covered there — platform/pulse/src/delivery-worker.test.ts
# ("DeliveryWorker permanent failure path") asserts a failed inject emits
# nudge.surface.failed with permanent=true and marks the row failed, never
# nudge.surfaced. That is the same contract, tested against the code that
# actually runs.
#
# What remains here is the absence check, so the retired path cannot quietly
# come back underneath a name that used to mean something.

@test "the bash nudge script stays retired (#2804/#2809)" {
  [ ! -e "${CHORUS_ROOT}/platform/scripts/nudge" ]
}

@test "chorus-hooks nudge.rs stays retired (#2804/#2809)" {
  [ ! -e "${CHORUS_ROOT}/platform/services/chorus-hooks/src/nudge.rs" ]
}

@test "the delivered-vs-failed invariant is still covered in pulse" {
  # Guards the real risk of retiring the old tests: that the invariant quietly
  # loses its only coverage. If this file moves or the case is renamed, fail
  # loudly here rather than discovering the gap after a silent misdelivery.
  local worker_test="${CHORUS_ROOT}/platform/pulse/src/delivery-worker.test.ts"
  [ -f "$worker_test" ]
  grep -q 'DeliveryWorker permanent failure path' "$worker_test"
  grep -q 'nudge.surface.failed' "$worker_test"
}
