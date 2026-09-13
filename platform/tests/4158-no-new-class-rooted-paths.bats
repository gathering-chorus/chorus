#!/usr/bin/env bats
# @test-type: fitness — a repo-wide ratchet over source text; no service, no store, no network.
# #4158 — RATCHET: the number of class-rooted athena-make paths in the repo may
# never grow. Every generated collection is /<domain>/<segment> (/code/files,
# /tests/results, /logs/sources); the class-rooted form (/codefiles,
# /testresults, /logsources) answers only as a deprecated alias while the
# remaining callers move. This guard is what stops a new one being written.
#
# The count is the LIVE caller count, not zero: athena-make's own source and its
# hermetic tests state both forms on purpose (the alias is a feature under test),
# and werk-test's three URLs move inside #4154, which is editing that same file.

CLASS_ROOTED='/(codefiles|codekinds|testresults|testsuiteruns|logsources)\b'
CEILING=76

count_hits() {
  cd "${CHORUS_ROOT:-$BATS_TEST_DIRNAME/../..}" || return 1
  grep -rnoE "$CLASS_ROOTED" \
    --include='*.ts' --include='*.js' --include='*.sh' --include='*.py' \
    --include='*.rs' --include='*.bats' --include='*.yml' . 2>/dev/null \
    | grep -vE 'node_modules|/dist/|target/|chorus-werk|4158-no-new-class-rooted' \
    | wc -l | tr -d ' '
}

@test "no new class-rooted athena-make path enters the repo" {
  n=$(count_hits)
  [ "$n" -le "$CEILING" ] || {
    echo "class-rooted paths: $n, ceiling $CEILING — a new one was written."
    echo "Use the domain-rooted path: /code/files, /tests/results, /logs/sources."
    false
  }
}

@test "NEGATIVE PROOF: the guard fails when a class-rooted path is added" {
  # Write one into a scratch file inside the tree, prove the count rises past
  # the ceiling, then remove it. A guard that cannot go red is not a guard.
  cd "${CHORUS_ROOT:-$BATS_TEST_DIRNAME/../..}"
  before=$(count_hits)
  probe="platform/tests/.4158-probe-$$.sh"
  printf 'curl -s localhost:3360/codefiles\ncurl -s localhost:3360/testresults\n' > "$probe"
  after=$(count_hits)
  rm -f "$probe"
  [ "$after" -gt "$before" ] || { echo "guard is blind: $before → $after"; false; }
  [ "$after" -gt "$CEILING" ] || { echo "ceiling $CEILING is slack: $after still under it"; false; }
  # and the tree is clean again
  [ "$(count_hits)" -eq "$before" ]
}
