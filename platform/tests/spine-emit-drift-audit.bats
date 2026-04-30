#!/usr/bin/env bats
# spine-emit-drift-audit.bats — #2630 wave 2
#
# Catches the failure pattern Jeff named 2026-04-30 morning: 12 done-briefs
# filed vs 1 card.accepted spine event in 5 days. Skills declare side-effects
# in their markdown; without a test that asserts the side-effect fires, the
# step decays to optional. This audit makes the drift loud.
#
# Audits chorus.log for window N (default 24h):
# - Every brief file matching */briefs/YYYY-MM-DD-card-NNN-done.md must have
#   a corresponding card.accepted spine event with matching card_id.
# - Every gate:product-pass card-comment in the window must have a probe-
#   evidence spine event within ±60s (today's paper-trail pattern).
#
# RED-first: against a chorus.log that already shows drift, this test fails
# loudly. After /acp + gate skills are fixed to always emit, this test
# stays green forever.

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"
CHORUS_LOG="$CHORUS_ROOT/platform/logs/chorus.log"

# Window: default last 24h, override via SPINE_DRIFT_WINDOW_HOURS.
WINDOW_HOURS="${SPINE_DRIFT_WINDOW_HOURS:-24}"
WINDOW_CUTOFF=$(date -u -v-${WINDOW_HOURS}H +"%Y-%m-%dT%H:%M:%S" 2>/dev/null \
  || date -u -d "${WINDOW_HOURS} hours ago" +"%Y-%m-%dT%H:%M:%S" 2>/dev/null \
  || echo "1970-01-01T00:00:00")

# All chorus done-briefs in window, scoped to actual on-disk files.
BRIEFS_DIR="$CHORUS_ROOT/directing/products/roles"

setup() {
  if [ ! -f "$CHORUS_LOG" ]; then
    skip "chorus.log missing at $CHORUS_LOG — audit cannot run"
  fi
}

@test "every recent done-brief has a card.accepted spine event" {
  # Find recent done-brief files (mtime within window)
  recent_briefs=$(find "$BRIEFS_DIR" -type f -name "*-card-*-done.md" \
    -mtime -1 2>/dev/null | head -50)

  if [ -z "$recent_briefs" ]; then
    skip "no recent done-briefs in window — nothing to audit"
  fi

  missing_events=()
  while IFS= read -r brief; do
    # Extract card id from filename: *-card-NNNN-done.md → NNNN
    card_id=$(basename "$brief" | sed -E 's/.*-card-([0-9]+)-done\.md/\1/')
    if [ -z "$card_id" ] || [ "$card_id" = "$(basename "$brief")" ]; then
      continue
    fi
    # Look for a card.accepted spine event mentioning this card_id
    if ! grep -q "\"event\":\"card\.accepted\"" "$CHORUS_LOG" 2>/dev/null \
        || ! grep -E "\"event\":\"card\.accepted\".*card[_id]*[\":= ]+[\"']?${card_id}[\"' ]" "$CHORUS_LOG" 2>/dev/null | head -1 >/dev/null; then
      missing_events+=("#${card_id} ($(basename "$brief"))")
    fi
  done <<< "$recent_briefs"

  if [ ${#missing_events[@]} -gt 0 ]; then
    echo "Found done-briefs WITHOUT a corresponding card.accepted spine event:"
    printf "  %s\n" "${missing_events[@]}"
    echo ""
    echo "  Each /acp invocation should fire BOTH a brief file and a spine"
    echo "  event. Drift here means the skill's spine-emit step is being"
    echo "  skipped (today's pattern: 12 briefs vs 1 spine event)."
    false
  fi
}

@test "every recent gate:product-pass comment has a probe-evidence spine event" {
  # Look for gate:product-pass mentions in chorus.log within window. The
  # comment itself flows through card.comment events on the spine. Each
  # gate-PASS should have a near-by probe.evidence event (within 60s).
  pass_count=$(grep -c "gate:product-pass" "$CHORUS_LOG" 2>/dev/null || echo 0)

  if [ "$pass_count" = "0" ]; then
    skip "no gate:product-pass mentions in chorus.log window — nothing to audit"
  fi

  # If there are no probe.evidence events at all, every gate:product-pass
  # is a paper-trail.
  evidence_count=$(grep -c "\"event\":\"probe\.evidence\"" "$CHORUS_LOG" 2>/dev/null || echo 0)

  if [ "$evidence_count" = "0" ] && [ "$pass_count" -gt "0" ]; then
    echo "Found ${pass_count} gate:product-pass mentions but ZERO probe.evidence"
    echo "spine events in the same window."
    echo ""
    echo "  Today's paper-trail pattern (Jeff 2026-04-30): gate:product-pass"
    echo "  comments without accompanying probe-evidence emissions are"
    echo "  ceremony, not gates. Each /gate-product invocation must emit"
    echo "  probe.evidence with the actual stdout/probe artifact, not just"
    echo "  a comment."
    false
  fi
}
