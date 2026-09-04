#!/usr/bin/env bats
# @test-type: unit — reads the git index and the ignore rules; no service, no network
# Subject: runtime artifacts must never be tracked in this repo.
#
# ADR-041: logs are runtime, not source. #3388 took the big log directories out
# of the tree because 28 of 35 leaked-credential findings were in committed log
# files. It left six behind and its acceptance boxes were never checked, so the
# gap sat open: platform/pulse/logs/ and a bare *.log at the repo root are both
# still writable-and-trackable.
#
# Clearing the credentials out of git HISTORY (#3386) is only worth doing once
# the working tree has stopped producing new ones. This suite is what keeps it
# stopped.
load test_helper

@test "no runtime log or board snapshot is tracked" {
  cd "$CHORUS_ROOT"
  run bash -c "git ls-files | grep -E '\.log$|/logs/.*\.(log|json)$|board-snapshot-.*\.json$'"
  [ "$status" -ne 0 ] || {
    echo "still tracked:"
    echo "$output"
    false
  }
}

@test "the ignore rules cover the paths #3388 missed" {
  cd "$CHORUS_ROOT"
  for p in platform/pulse/logs/messaging.log rust-analyzer-stats.log platform/logs/board-snapshot-gathering-silas.json; do
    run git check-ignore -q "$p"
    [ "$status" -eq 0 ] || {
      echo "$p is NOT ignored — a service writing there gets committed"
      false
    }
  done
}

@test "NEGATIVE PROOF: the rules are not a blanket ignore" {
  # Without this, the test above passes just as well against a .gitignore
  # containing a single '*' — which would hide every source file in the repo
  # and report the same green. Show a normal source path is still visible.
  cd "$CHORUS_ROOT"
  run git check-ignore -q platform/scripts/deep-health.sh
  [ "$status" -ne 0 ]
  run git check-ignore -q platform/tests/tracked-runtime-logs.bats
  [ "$status" -ne 0 ]
}

@test "NEGATIVE PROOF: a NEW log at a covered path is ignored, not silently trackable" {
  # The rules above are asserted against paths that already exist. The thing we
  # actually care about is the next file, so make one.
  cd "$CHORUS_ROOT"
  local f="platform/pulse/logs/fixture-$$-probe.log"
  mkdir -p platform/pulse/logs
  echo "fixture" > "$f"
  run git check-ignore -q "$f"
  local rc="$status"
  rm -f "$f"
  [ "$rc" -eq 0 ]
}
