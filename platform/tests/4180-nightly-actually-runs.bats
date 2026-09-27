#!/usr/bin/env bats
# @test-type: contract
# @domain: code — the product domain this suite guards (#4334)
# #4180 — "the nightly ran" must be a check that can go red.
#
# #4178 landed com.chorus.crawl-nightly.plist in the repo and reported the
# crawler self-maintaining. It was never installed: not in LaunchAgents, not
# loaded, no log. Nothing said so, because nothing checked. This suite asks the
# box, not the repo — and its negative proof unloads the unit (via a PATH shim)
# to show the check fails on exactly the state that shipped.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  UNIT=com.chorus.crawl-nightly
  LOG="${CRAWL_NIGHTLY_LOG:-$HOME/Library/Logs/Chorus/crawl-nightly.log}"
}

# the check itself: loaded on THIS box, asked of launchctl, never of a file list
unit_loaded() { launchctl list 2>/dev/null | grep -q "$UNIT"; }

@test "the nightly unit is loaded on this box" {
  run unit_loaded
  [ "$status" -eq 0 ]
}

# NEGATIVE PROOF (#3734): shim launchctl so the unit is absent from its list —
# the state #4178 shipped in — and the same check must fail.
@test "NEGATIVE PROOF: with the unit unloaded the check goes red" {
  shim="$BATS_TEST_TMPDIR/bin"; mkdir -p "$shim"
  printf '#!/bin/sh\necho "-\t0\tcom.chorus.something-else"\n' > "$shim/launchctl"; chmod +x "$shim/launchctl"
  PATH="$shim:$PATH" run unit_loaded
  [ "$status" -ne 0 ]
}

# The log is where "a pass nobody started" will show. Before the first 04:30
# there is no log — that is reported as such, never ticked early (Silas,
# 2026-09-16 08:54). The morning line reads this same file (crawl_line).
@test "the nightly log, when it exists, holds a real pass and not only noise" {
  [ -f "$LOG" ] || skip "no nightly pass yet — the unit fires 04:30; do not tick AC1 early"
  run grep -cE 'chorus-crawl: (wrote=|reconcile:|watermark)' "$LOG"
  [ "$output" -ge 1 ]
}
