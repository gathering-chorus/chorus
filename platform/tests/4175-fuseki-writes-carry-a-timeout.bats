#!/usr/bin/env bats
# @test-type: fitness — a repo-wide check over source text; no service, no store, no network.
#
# #4175 — every WRITE to Fuseki carries a timeout.
#
# 2026-09-14: a compaction held the store's write lock from 15:01. A bare
# `curl -X DELETE` against the staging graph sat for 32 minutes and took a whole
# run with it, silently — the log's last line was 29 minutes old and the run
# still read as "in progress". Reads answered in 0.011s the entire time, so the
# store was fine; the lock was held. Silas, who owns the store: "a held lock
# should fail a test in seconds, not hang it."
#
# An unbounded write turns "someone else is holding the lock" into a hung run
# nobody can read, instead of a failure that names itself. This guard is
# source-shaped on purpose: the condition it catches is a line of code, and the
# alternative — noticing the hang — is what cost the 32 minutes.
#
# ROOT: the tree this check travels with, from its own location, never
# $CHORUS_ROOT — a fitness check pointed at a different checkout than the diff
# it guards says nothing true about the change under test (#4158's lesson).

ROOT="$BATS_TEST_DIRNAME/../.."

# A WRITE is a curl carrying an explicit method AND addressed to the graph store.
# Two deliberate narrowings, so the guard means what the incident meant:
#   - reads have no -X and are out of scope. They answered in 0.011s throughout;
#     putting a deadline on a long legitimate query is a different decision with
#     a different owner.
#   - a curl with -X at some other HTTP API (the MCP, the nudge bridge, a CORS
#     probe) is not a Fuseki write and cannot be held by this lock. Sweeping
#     those in would have made this a 90-site refactor wearing an incident's
#     name, which is how a guard ends up too big to keep true.
STORE='\$GSP|\$\{?FUSEKI|:3030|/pods/|\$STAGING|sparql-update'
writes_without_timeout() {
  grep -rn --include='*.sh' --include='*.bats' -E 'curl[^|]*[[:space:]]-X[[:space:]]' \
    "$ROOT/platform/scripts" "$ROOT/platform/tests" 2>/dev/null \
    | grep -v -- '--max-time' \
    | grep -E "$STORE" \
    | grep -v '4175-fuseki-writes-carry-a-timeout.bats' || true
}

@test "#4175 no Fuseki write is unbounded" {
  run writes_without_timeout
  [ -z "$output" ] || { echo "untimed write call(s):"; echo "$output"; return 1; }
}

@test "#4175 NEGATIVE PROOF: the guard fails on a violation" {
  # The guard must be shown to separate the two states, not merely to pass
  # (#3734). A file carrying exactly the shape it exists to catch must be found.
  local bad="$BATS_TEST_TMPDIR/violation.sh"
  mkdir -p "$BATS_TEST_TMPDIR/platform/scripts" "$BATS_TEST_TMPDIR/platform/tests"
  bad="$BATS_TEST_TMPDIR/platform/scripts/violation.sh"
  echo 'curl -s -X DELETE "$GSP?graph=$G" -o /dev/null' > "$bad"
  run env ROOT="$BATS_TEST_TMPDIR" STORE="$STORE" bash -c '
    grep -rn --include="*.sh" --include="*.bats" -E "curl[^|]*[[:space:]]-X[[:space:]]" \
      "$ROOT/platform/scripts" "$ROOT/platform/tests" 2>/dev/null | grep -v -- "--max-time" | grep -E "$STORE" || true'
  [ -n "$output" ] || { echo "the guard did not find a planted untimed write"; return 1; }
  # And it must NOT flag the same line once the timeout is there — otherwise it
  # cannot tell the fixed state from the broken one.
  echo 'curl -s --max-time "${FUSEKI_WRITE_TIMEOUT:-120}" -X DELETE "$GSP?graph=$G" -o /dev/null' > "$bad"
  run env ROOT="$BATS_TEST_TMPDIR" STORE="$STORE" bash -c '
    grep -rn --include="*.sh" --include="*.bats" -E "curl[^|]*[[:space:]]-X[[:space:]]" \
      "$ROOT/platform/scripts" "$ROOT/platform/tests" 2>/dev/null | grep -v -- "--max-time" | grep -E "$STORE" || true'
  [ -z "$output" ] || { echo "the guard flags a timed write: $output"; return 1; }
}

@test "#4175 the timeout is overridable, not hard-coded" {
  # A deliberately long deploy must have a way through that is not deleting the
  # guard. Every timed call reads the same knob.
  run grep -rhoE '\-\-max-time "\$\{FUSEKI_WRITE_TIMEOUT:-[0-9]+\}"' \
    "$ROOT/platform/scripts/athena-deploy-model.sh"
  [ "$status" -eq 0 ]
  [ -n "$output" ]
}
