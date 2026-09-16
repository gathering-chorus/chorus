#!/usr/bin/env bats
# @test-type: unit
# #4192 — the land's crawl step never sits on the critical path.
#
# Jeff, 2026-09-16 18:11: "i did not want an extra 10 minutes on every werk."
# hermetic: a stub crawler that sleeps stands in for the binary; no store.

# bash 3.2 never fires errexit on a failing `[[ ]]` (see 4185 suites); simple commands only.
has() { grep -qF -- "$1" <<<"${2-$output}"; }

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  SCRIPT="$REPO/platform/scripts/crawl-detached.sh"
  STUB="$BATS_TEST_TMPDIR/chorus-crawl"
  printf '#!/bin/sh\necho "chorus-crawl: full (stub) · tracked=1 read=Complete"\nsleep 6\necho "chorus-crawl: wrote=0 failed=0"\n' > "$STUB"
  chmod +x "$STUB"
  export CHORUS_CRAWL_BIN="$STUB" CHORUS_CRAWL_LOG_DIR="$BATS_TEST_TMPDIR/runs" CARD_ID=4192 ROLE=kade
}

@test "the land step returns at once while the crawler keeps running in its own log" {
  start=$(date +%s)
  run bash "$SCRIPT"
  end=$(date +%s)
  [ "$status" -eq 0 ]
  has "crawl-detached: started chorus-crawl"
  [ $((end - start)) -lt 5 ]
  log=$(ls "$BATS_TEST_TMPDIR"/runs/4192-kade-crawl-*.log | head -1)
  [ -n "$log" ]
  pid=$(cat "$log.pid")
  kill -0 "$pid"                                  # still running after the step returned
  sleep 7
  has "chorus-crawl: wrote=0 failed=0" "$(cat "$log")"   # and it finished into its log
}

# NEGATIVE PROOF (#3734): the timing check separates its two states — the same
# stub run in the FOREGROUND (the old step shape) takes the stub's full 6 s.
@test "NEGATIVE PROOF: the old inline shape would have held the land for the crawl's whole duration" {
  start=$(date +%s)
  run "$STUB"
  end=$(date +%s)
  [ $((end - start)) -ge 6 ]
}

@test "no crawler installed is said, not silently green" {
  export CHORUS_CRAWL_BIN="$BATS_TEST_TMPDIR/does-not-exist"
  run bash "$SCRIPT"
  [ "$status" -eq 0 ]
  has "not installed"
}

@test "the workflow runs the crawl AFTER accept and through the detached script" {
  yml="$REPO/.github/workflows/werk.yml"
  accept=$(grep -n 'name: accept$' "$yml" | cut -d: -f1)
  crawl=$(grep -n 'name: crawl-delta$' "$yml" | cut -d: -f1)
  [ -n "$accept" ]; [ -n "$crawl" ]
  [ "$crawl" -gt "$accept" ]
  run grep -A 6 'name: crawl-delta$' "$yml"
  has "crawl-detached.sh"
}
