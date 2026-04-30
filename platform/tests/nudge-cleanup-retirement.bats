#!/usr/bin/env bats
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

@test "GET /api/nudge/:role/pending stays retired" {
  # The route definition was at pulse/src/service.ts. Any new
  # app.get('/api/nudge/...pending'...) reintroduces it.
  matches=$(grep -rn "app\.get.*['\"]\/api\/nudge.*pending" $PROD_SRC 2>/dev/null \
    | grep $NOT_COMMENT_LINE \
    || true)
  if [ -n "$matches" ]; then
    echo "Found re-introduction of GET /api/nudge/:role/pending route:"
    echo "$matches"
    false
  fi
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
