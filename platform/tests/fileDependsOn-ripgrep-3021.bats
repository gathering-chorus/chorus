#!/usr/bin/env bats
# #3021: fileDependsOn must derive edges from ONE filesystem ripgrep pass
# (not a full ~6000-File scan + grep-each-file, which cost 33s) AND exclude
# dist/build output (the 13 false positives included dist artifacts).
# RED against the pre-#3021 version, GREEN after the rewrite.

SCRIPT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../scripts" && pwd)/enrichment-write-fileDependsOn.sh"

@test "no unbounded full chorus:File scan (the 33s cost)" {
  run grep -qF 'SELECT ?f ?p WHERE' "$SCRIPT"
  [ "$status" -ne 0 ]
}

@test "excludes dist/build output (the dist false-positives)" {
  run grep -qF 'dist' "$SCRIPT"
  [ "$status" -eq 0 ]
}

# blast-radius = "who breaks if spine changes" — that includes spine READERS,
# not just emitters. The emit-only pattern missed context-spine.ts / chorus-rcas.ts
# (they consume spine without emitting). The pattern must carry consumer tokens.
@test "captures spine CONSUMERS, not just emitters (reader blast-radius)" {
  run grep -qE 'SpineEntry|SpineEvent|spineLogPath|chorusLogPath|spine_events' "$SCRIPT"
  [ "$status" -eq 0 ]
}
