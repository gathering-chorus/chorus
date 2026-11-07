#!/usr/bin/env bats
# @test-type: unit
# @domain: autonomy
# @card: #4916 · owner: silas
# #4916 — the stated-intent check reads platform/config/stated-intent-vocabulary.json.
# The check's logic is proved in chorus-hooks tests/stated_intent_gemini_4916.rs;
# this proves the VOCABULARY itself: its patterns catch Borg Queen's 16:13 closing
# line (a promise after tool calls) and her "Advancing" line, and pass a done-report.
# A vocabulary whose patterns stop matching the promise shape goes red here.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  VOCAB="$REPO/platform/config/stated-intent-vocabulary.json"
}

# matches <closing line> → prints the ids of the commitments whose pattern matches
matches() {
  python3 - "$VOCAB" "$1" <<'PY'
import json, re, sys
vocab, line = sys.argv[1], sys.argv[2]
for c in json.load(open(vocab))["commitments"]:
    if re.search(c["pattern"], line):
        print(c["id"])
PY
}

@test "the 16:13 closing promise is caught by bare-gerund" {
  run matches "Executing slice 1 now."
  [ "$status" -eq 0 ]
  [[ "$output" == *"bare-gerund"* ]] || { echo "not caught: $output"; return 1; }
}

@test "an 'Advancing to Slice 3.' promise is caught" {
  run matches "Advancing to Slice 3."
  [[ "$output" == *"bare-gerund"* ]] || { echo "not caught: $output"; return 1; }
}

@test "negative proof: a done-report is not caught by any commitment" {
  run matches "Nudged Wren."
  [ "$status" -eq 0 ]
  [ -z "$output" ] || { echo "a done-report was caught as a promise: $output"; return 1; }
}
