#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
# nudge-cleanup-retirement.bats — #2628 retirement-gate (#2630 AC delivery)
#
# Forward-only structural assertion of the #2628 retirement decisions.
# Asserts that the nudge-history-ack helper family + dead alert/script
# stay gone in production code. If a future PR re-introduces any of
# these surfaces, this test fails before merge.
#
# Same shape as #2467 (role-state-card-decoupled.bats), #2629/#2632
# (role-state HTTP retirement gates) — the family Silas anchored.
#
# Comments are excluded — retirement-note comments referencing the old
# names are expected and harmless.

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"

# Production source tree (excludes tests/coverage/dist/node_modules).
PROD_SRC="$CHORUS_ROOT/platform/pulse/src $CHORUS_ROOT/platform/services/chorus-hooks/src $CHORUS_ROOT/platform/api/src"

# Comment-strip regex: //, /* */, #, *
NOT_COMMENT_LINE='-v -E ^[[:space:]]*(//|#|\*|/\*)'

@test "GET /api/nudge/:role/pending stays retired IN PULSE" {
  # NARROWED 2026-08-25 (#4006), deliberately, with the reason recorded:
  #
  # #2664 retired PULSE's implementation of this route — a count with no
  # identity behind it, served off a store that has since gone. #2725 then
  # rebuilt the surface in chorus-api against the spine fold, with a declared
  # role lane (401 authn-missing / 403 no-role-held / 403 not-your-lane) and a
  # negative proof that a transient surface-failure does not clear a pending
  # nudge. Jeff accepted it on 2026-08-25; the route is wanted.
  #
  # So the guard was asserting the wrong thing: it read as "this PATH may never
  # exist" when the decision was "pulse may not own it". Left as it was, the
  # only ways to go green were to delete a route Jeff had just accepted or to
  # delete the guard — and the guard is worth keeping, because the pulse
  # implementation coming back IS still a regression.
  #
  # Scope is therefore pulse/src alone. chorus-api owning the route is the
  # current design, not a re-introduction.
  matches=$(grep -rn "app\.get.*['\"]\/api\/nudge.*pending" "$CHORUS_ROOT/platform/pulse/src" 2>/dev/null \
    | grep $NOT_COMMENT_LINE \
    || true)
  if [ -n "$matches" ]; then
    echo "Found re-introduction of GET /api/nudge/:role/pending in PULSE:"
    echo "$matches"
    false
  fi
}

# NEGATIVE PROOF for the narrowing — the guard must still catch the thing it
# was built for. A pulse-shaped route line is grepped with the SAME pattern and
# must match; without this, "narrowed to pulse/src" could quietly mean "matches
# nothing anywhere" and the gate would pass vacuously forever (#3734).
@test "the pulse-scoped guard still REDS on a pulse re-introduction" {
  fixture="app.get('/api/nudge/:role/pending', async (req, res) => {"
  echo "$fixture" | grep -q -E "app\.get.*['\"]/api/nudge.*pending"

  # ...and does NOT fire on the chorus-api module that legitimately owns it now,
  # so the two states stay distinguishable.
  legit="import { decideNudgePending } from './nudge-pending-route';"
  ! echo "$legit" | grep -q -E "app\.get.*['\"]/api/nudge.*pending"
}

@test "GET /api/dead-letter stays retired" {
  matches=$(grep -rn "app\.get.*['\"]\/api\/dead-letter['\"]" $PROD_SRC 2>/dev/null \
    | grep $NOT_COMMENT_LINE \
    || true)
  if [ -n "$matches" ]; then
    echo "Found re-introduction of GET /api/dead-letter route:"
    echo "$matches"
    false
  fi
}

@test "POST /api/dead-letter/:id/replay stays retired" {
  matches=$(grep -rn "app\.post.*['\"]\/api\/dead-letter.*replay" $PROD_SRC 2>/dev/null \
    | grep $NOT_COMMENT_LINE \
    || true)
  if [ -n "$matches" ]; then
    echo "Found re-introduction of POST /api/dead-letter/:id/replay route:"
    echo "$matches"
    false
  fi
}

@test "getPendingNudges helper stays retired" {
  matches=$(grep -rn "^[[:space:]]*\(public\|private\|export\)\?[[:space:]]*getPendingNudges\b" $PROD_SRC 2>/dev/null \
    | grep $NOT_COMMENT_LINE \
    || true)
  if [ -n "$matches" ]; then
    echo "Found re-introduction of getPendingNudges helper:"
    echo "$matches"
    false
  fi
}

@test "acknowledgeNudge / acknowledgeAllNudges helpers stay retired" {
  matches=$(grep -rnE "^[[:space:]]*(public|private|export)?[[:space:]]*(acknowledgeNudge|acknowledgeAllNudges)\b" $PROD_SRC 2>/dev/null \
    | grep $NOT_COMMENT_LINE \
    || true)
  if [ -n "$matches" ]; then
    echo "Found re-introduction of acknowledge* helpers:"
    echo "$matches"
    false
  fi
}

@test "recordDeliveryAttempt / replayDeadLetter / getDeadLetters helpers stay retired" {
  matches=$(grep -rnE "^[[:space:]]*(public|private|export)?[[:space:]]*(recordDeliveryAttempt|replayDeadLetter|getDeadLetters)\b" $PROD_SRC 2>/dev/null \
    | grep $NOT_COMMENT_LINE \
    || true)
  if [ -n "$matches" ]; then
    echo "Found re-introduction of dead-letter helpers:"
    echo "$matches"
    false
  fi
}

@test "proving/scripts/inject-watcher.sh stays deleted" {
  if [ -f "$CHORUS_ROOT/proving/scripts/inject-watcher.sh" ]; then
    echo "inject-watcher.sh re-appeared at proving/scripts/inject-watcher.sh"
    echo "  This script was retired by #2435 (LaunchAgent unloaded) and"
    echo "  the file deleted by #2628. The canonical receiver is"
    echo "  spine-tick-poller. Don't re-introduce."
    false
  fi
}

@test "proving/domains/alerts/nudge-stale.yml stays deleted" {
  if [ -f "$CHORUS_ROOT/proving/domains/alerts/nudge-stale.yml" ]; then
    echo "nudge-stale.yml re-appeared at proving/domains/alerts/nudge-stale.yml"
    echo "  This alert was retired by #2628 — it was the source of 13"
    echo "  nudge-stale alerts/day reading /tmp/voice-inbox/<role>/"
    echo "  pending-inject.txt mtime, which #2435 retired the writer for."
    echo "  If pending-count alerting earns its keep again, source from"
    echo "  the spine fold (nudge.emitted minus nudge.surfaced)."
    false
  fi
}

@test "voice-inbox path-checks stay retired in production code" {
  # /tmp/voice-inbox/<role>/pending-inject.txt was the queue file from
  # the inject-watcher model retired by #2435. #2628 removed remaining
  # code that read from it (pulse.rs assemble_nudges).
  matches=$(grep -rn "\/tmp\/voice-inbox.*pending-inject" $PROD_SRC 2>/dev/null \
    | grep $NOT_COMMENT_LINE \
    || true)
  if [ -n "$matches" ]; then
    echo "Found re-introduction of /tmp/voice-inbox path-check in production:"
    echo "$matches"
    echo "  voice-inbox is retired. Pending count comes from spine fold."
    false
  fi
}
