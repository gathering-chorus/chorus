#!/usr/bin/env bats
# @test-type: fitness — a repo-wide ratchet over source text; no service, no store, no network.
# #4158 — RATCHET: the number of class-rooted athena-make paths in the repo may
# never grow. Every generated collection is /<domain>/<segment> (/code/files,
# /tests/results, /logs/sources); the class-rooted form (/codefiles,
# /testresults, /logsources) answers only as a deprecated alias while the
# remaining callers move. This guard is what stops a new one being written.
#
# 2026-09-13: the ceiling ROSE 76 -> 79 over this card, which a ratchet should
# normally refuse. Stated plainly rather than tuned away: this guard counts
# MENTIONS, not callers, and it cannot tell a live call from an assertion that
# names the deprecated path on purpose. The rise is entirely the latter — the
# negative proofs must name /v1/testresults to assert it is NOT handed back.
# Meanwhile the one real caller that moved (4157 bats) went 5 -> 1.
# So this guard stops NEW class-rooted paths; it cannot certify that all
# callers have moved. AC4's second half needs a call-shaped check, not this.
# The additions are all deliberate and all in checks: the negative proofs must
# NAME the deprecated path to assert it is not handed back, and werk-test's
# writeback stays on the alias until #4158 reaches canonical (run 77 lost 650
# results proving why). Deliberate mentions in assertions are not new callers.
#
# The count is the LIVE caller count, not zero: athena-make's own source and its
# hermetic tests state both forms on purpose (the alias is a feature under test),
# and werk-test's three URLs move inside #4154, which is editing that same file.

CLASS_ROOTED='/(codefiles|codekinds|testresults|testsuiteruns|logsources)\b'
CEILING=79

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
