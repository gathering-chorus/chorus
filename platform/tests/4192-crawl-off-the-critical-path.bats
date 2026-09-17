#!/usr/bin/env bats
# @test-type: unit
# #4192 / #4199 — the land's crawl step never sits on the critical path, and the
# process it starts is launchd's, not the job's.
#
# hermetic: CRAWL_KICKSTART_CMD is handed a stub; no launchd, no store.

has() { grep -qF -- "$1" <<<"${2-$output}"; }

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  SCRIPT="$REPO/platform/scripts/crawl-detached.sh"
  STUB="$BATS_TEST_TMPDIR/kickstart"
  printf '#!/bin/sh\necho "$@" > "%s/called"\n' "$BATS_TEST_TMPDIR" > "$STUB"
  chmod +x "$STUB"
}

@test "the land step hands the crawl to launchd and returns at once" {
  start=$(date +%s)
  CRAWL_KICKSTART_CMD="$STUB" run bash "$SCRIPT"
  end=$(date +%s)
  [ "$status" -eq 0 ]
  has "kickstarted com.chorus.crawl-nightly"
  [ $((end - start)) -lt 5 ]
  [ -f "$BATS_TEST_TMPDIR/called" ]
}

# NEGATIVE PROOF (#3734): the timing check separates its two states — a command
# that blocks for the crawl's duration (the pre-#4192 inline shape) is caught.
@test "NEGATIVE PROOF: an inline crawl would hold the step for its whole duration" {
  printf '#!/bin/sh\nsleep 6\n' > "$BATS_TEST_TMPDIR/slow"; chmod +x "$BATS_TEST_TMPDIR/slow"
  start=$(date +%s)
  CRAWL_KICKSTART_CMD="$BATS_TEST_TMPDIR/slow" run bash "$SCRIPT"
  end=$(date +%s)
  [ $((end - start)) -ge 6 ]
}

@test "a failed kickstart is said, not silently green, and does not fail the land" {
  printf '#!/bin/sh\necho "Could not find service"; exit 113\n' > "$BATS_TEST_TMPDIR/fail"; chmod +x "$BATS_TEST_TMPDIR/fail"
  CRAWL_KICKSTART_CMD="$BATS_TEST_TMPDIR/fail" run bash "$SCRIPT"
  [ "$status" -eq 0 ]
  has "kickstart of com.chorus.crawl-nightly failed"
  has "Could not find service"
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
