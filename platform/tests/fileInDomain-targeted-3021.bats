#!/usr/bin/env bats
# #3021: fileInDomain must tag by TARGETED lookup, not a full ~6000-File scan.
# The pre-#3021 version queried every chorus:File ("SELECT ?f ?p WHERE ... ?f a
# chorus:File ; chorus:filePath ?p") and looped all rows — 21s to tag 5 files.
# These assertions are RED against that version, GREEN after the targeted rewrite.

SCRIPT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../scripts" && pwd)/enrichment-write-fileInDomain.sh"

@test "no unbounded full chorus:File scan (the 21s cost)" {
  run grep -qF 'SELECT ?f ?p WHERE' "$SCRIPT"
  [ "$status" -ne 0 ]
}

@test "uses targeted per-core-file lookup (STRENDS suffix match)" {
  run grep -qF 'STRENDS(STR(?p)' "$SCRIPT"
  [ "$status" -eq 0 ]
}
