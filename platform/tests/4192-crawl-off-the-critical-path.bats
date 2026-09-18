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
  # Read the STEP, not a fixed window. `grep -A 6` measured six lines and
  # called that the step: adding a four-line comment to the step on 2026-09-18
  # pushed `run:` out of view and reded this test with the workflow unchanged.
  # awk from this step's name to the next one, however long it is.
  run awk '/name: crawl-delta$/{f=1} f&&/^      - name: /&&!/crawl-delta/{exit} f' "$yml"
  has "crawl-detached.sh"
}

# NEGATIVE PROOF (#3734) for the change above: a window read cannot tell a step
# that lost its `run:` from a step that merely grew a comment. The derived read
# can — it still REDS when the line is actually gone.
@test "NEGATIVE PROOF: the step read reds when run: is actually missing" {
  yml="$BATS_TEST_TMPDIR/werk.yml"
  { echo "      - name: crawl-delta"
    echo "        continue-on-error: true"
    echo "        env:"
    for i in 1 2 3 4 5 6 7 8; do echo "          # padding line $i"; done
    echo "          CHORUS_ROLE: kade"
    echo "      - name: outcome"
    echo "        run: bash crawl-detached.sh"
  } > "$yml"
  # the padding alone must not hide a present run: — and the next step's
  # crawl-detached.sh must not be mistaken for this step's
  run awk '/name: crawl-delta$/{f=1} f&&/^      - name: /&&!/crawl-delta/{exit} f' "$yml"
  if grep -qF 'crawl-detached.sh' <<<"$output"; then
    echo "read past the step boundary" >&2; return 1
  fi
}
