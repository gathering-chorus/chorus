#!/usr/bin/env bats
# @test-type: unit
# @domain: cicd — the product domain this suite guards (#4334)
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
  CRAWL_STATE_CMD=true CRAWL_KICKSTART_CMD="$STUB" run bash "$SCRIPT"
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
  CRAWL_STATE_CMD=true CRAWL_KICKSTART_CMD="$BATS_TEST_TMPDIR/slow" run bash "$SCRIPT"
  end=$(date +%s)
  [ $((end - start)) -ge 6 ]
}

# #4185 — a land while a pass runs leaves it to finish. The old `kickstart -k`
# SIGTERMed a running full pass on every land (2026-09-30 14:31 → 15:27).
@test "a land while a pass is running leaves it running and says so" {
  printf '#!/bin/sh\nprintf "\\tstate = running\\n\\tpid = 4242\\n"\n' > "$BATS_TEST_TMPDIR/running"; chmod +x "$BATS_TEST_TMPDIR/running"
  CRAWL_STATE_CMD="$BATS_TEST_TMPDIR/running" CRAWL_KICKSTART_CMD="$STUB" CRAWL_LOG="$BATS_TEST_TMPDIR/crawl.log" run bash "$SCRIPT"
  [ "$status" -eq 0 ]
  has "already running (pid 4242) — left to finish"
  [ ! -f "$BATS_TEST_TMPDIR/called" ] || return 1
  grep -qF "left to finish" "$BATS_TEST_TMPDIR/crawl.log" || return 1
}

# NEGATIVE PROOF (#3734): the running check separates its two states — an idle
# job IS started, and the default start no longer carries -k.
@test "NEGATIVE PROOF: an idle job is started, and never with -k" {
  printf '#!/bin/sh\nprintf "\\tstate = not running\\n"\n' > "$BATS_TEST_TMPDIR/idle"; chmod +x "$BATS_TEST_TMPDIR/idle"
  CRAWL_STATE_CMD="$BATS_TEST_TMPDIR/idle" CRAWL_KICKSTART_CMD="$STUB" CRAWL_LOG="$BATS_TEST_TMPDIR/crawl.log" run bash "$SCRIPT"
  [ -f "$BATS_TEST_TMPDIR/called" ]
  run grep -E 'CMD="\$\{CRAWL_KICKSTART_CMD:-launchctl kickstart -k' "$SCRIPT"
  [ "$status" -ne 0 ]
}

@test "a failed kickstart is said, not silently green, and does not fail the land" {
  printf '#!/bin/sh\necho "Could not find service"; exit 113\n' > "$BATS_TEST_TMPDIR/fail"; chmod +x "$BATS_TEST_TMPDIR/fail"
  CRAWL_STATE_CMD=true CRAWL_KICKSTART_CMD="$BATS_TEST_TMPDIR/fail" run bash "$SCRIPT"
  [ "$status" -eq 0 ]
  has "kickstart of com.chorus.crawl-nightly failed"
  has "Could not find service"
}

