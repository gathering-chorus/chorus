#!/usr/bin/env bats
# @test-type: unit — signal is fixture-data: arq/generated files only; no live store, no $HOME, no network
# #3870 — doc-coherence must not probe ports it doesn't own. 3343/3344/3345
# are werk demo-variant chorus-api ports (demo_env.rs); a coherence probe that
# auto-detects them health-checks an unmerged branch. Negative proof: add a
# hardcoded 334[345] probe back to doc-coherence.sh and this goes red.

@test "doc-coherence.sh contains no werk-variant port probes" {
  DIR="$( cd "$( dirname "$BATS_TEST_FILENAME" )" && pwd )"
  SCRIPT="$DIR/../scripts/doc-coherence.sh"
  # code lines only — the retirement comment may name the ports (the #3725 trap)
  run bash -c "grep -v '^\s*#' '$SCRIPT' | grep -E 'localhost:334[345]|:3345'"
  [ "$status" -ne 0 ]
}
