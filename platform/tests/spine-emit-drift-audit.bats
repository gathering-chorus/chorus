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
  # Find recent done-brief files. mtime-based filtering was unreliable
  # across macOS/Linux (find -mtime semantics drift); pivot to filename
  # date-prefix matching instead. Briefs are named YYYY-MM-DD-card-NNNN-done.md.
  today=$(date -u +"%Y-%m-%d")
  yesterday=$(date -u -v-1d +"%Y-%m-%d" 2>/dev/null \
    || date -u -d "yesterday" +"%Y-%m-%d" 2>/dev/null \
    || echo "")

  recent_briefs=$(find "$BRIEFS_DIR" -type f -name "${today}-card-*-done.md" 2>/dev/null)
  if [ -n "$yesterday" ]; then
    yesterday_briefs=$(find "$BRIEFS_DIR" -type f -name "${yesterday}-card-*-done.md" 2>/dev/null)
    recent_briefs="${recent_briefs}
${yesterday_briefs}"
  fi
  recent_briefs=$(echo "$recent_briefs" | grep -v "^$" | head -50)

  if [ -z "$recent_briefs" ]; then
    skip "no done-briefs from today/yesterday — nothing to audit"
  fi

  missing_events=()
  while IFS= read -r brief; do
    # Extract card id from filename: *-card-NNNN-done.md → NNNN
    card_id=$(basename "$brief" | sed -E 's/.*-card-([0-9]+)-done\.md/\1/')
    if [ -z "$card_id" ] || [ "$card_id" = "$(basename "$brief")" ]; then
      continue
    fi
    # Look for a card.accepted spine event for THIS card_id with a
    # timestamp ≥ the brief's date (date-prefix in filename).
    # Per-subagent finding: unscoped grep matches stale events for the
    # same card_id from prior sessions — that's the false-pass hole.
    brief_date=$(basename "$brief" | grep -oE "^[0-9]{4}-[0-9]{2}-[0-9]{2}" | head -1)
    if [ -z "$brief_date" ]; then continue; fi

    found=$(grep "\"event\":\"card\.accepted\"" "$CHORUS_LOG" 2>/dev/null \
      | grep -E "card[_id]*[\":= ]+[\"']?${card_id}[\"' ]" \
      | grep -E "\"timestamp\":\"${brief_date}" \
      | head -1)

    if [ -z "$found" ]; then
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

@test "every recent gate:product-pass comment has a probe-evidence spine event within 60s" {
  # Per-comment correlation. For each gate:product-pass card-comment event
  # in chorus.log, find a probe.evidence event with the same card_id
  # within ±60s. Catches the paper-trail pattern (Jeff 2026-04-30 morning):
  # gate-PASS comment without accompanying probe emission.
  #
  # If gate-PASS events appear but NONE has correlated probe.evidence,
  # this is the catastrophic-drift case — fail loud.

  # Pull all card.comment events that mention gate:product-pass with their
  # timestamp + card_id. JSON shape varies; fall back to grep-and-parse.
  pass_lines=$(grep "\"event\":\"card\.comment\"" "$CHORUS_LOG" 2>/dev/null \
    | grep "gate:product-pass" \
    | head -20)

  if [ -z "$pass_lines" ]; then
    skip "no gate:product-pass card.comment events in chorus.log — nothing to audit"
  fi

  pass_total=0
  evidence_correlated=0
  uncorrelated=()

  while IFS= read -r pass_line; do
    pass_total=$((pass_total + 1))
    # Extract card_id from the line (best-effort against varying JSON shapes)
    pass_card=$(echo "$pass_line" | grep -oE "\"card[_id]*\":\"?[0-9]+\"?" \
      | head -1 | grep -oE "[0-9]+" | head -1)
    pass_ts=$(echo "$pass_line" | grep -oE "\"timestamp\":\"[^\"]+\"" \
      | head -1 | sed -E 's/.*"timestamp":"([^"]+)".*/\1/')

    if [ -z "$pass_card" ] || [ -z "$pass_ts" ]; then
      continue
    fi

    # Look for a probe.evidence event for this card within ±60s. Coarse
    # match: same card_id within ±60 lines (chorus.log emits ~1/sec under
    # load; 60 lines is a fair proxy for 60s).
    pass_lineno=$(grep -n "\"event\":\"card\.comment\"" "$CHORUS_LOG" \
      | grep "$pass_ts" | head -1 | cut -d: -f1)

    if [ -z "$pass_lineno" ]; then
      continue
    fi

    window_start=$((pass_lineno - 60))
    [ $window_start -lt 1 ] && window_start=1
    window_end=$((pass_lineno + 60))

    correlated=$(sed -n "${window_start},${window_end}p" "$CHORUS_LOG" 2>/dev/null \
      | grep "\"event\":\"probe\.evidence\"" \
      | grep -E "\"card[_id]*\":\"?${pass_card}\"?" \
      | head -1)

    if [ -n "$correlated" ]; then
      evidence_correlated=$((evidence_correlated + 1))
    else
      uncorrelated+=("#${pass_card} @ ${pass_ts}")
    fi
  done <<< "$pass_lines"

  # Threshold: drift fails if more than 50% are uncorrelated (per-subagent
  # finding: original `evidence_correlated == 0` gate was all-or-nothing —
  # 11/12 drift would pass silently. 50% threshold catches partial drift.)
  if [ "$pass_total" -gt 0 ]; then
    uncorrelated_count=$((pass_total - evidence_correlated))
    threshold=$((pass_total / 2))
    if [ "$uncorrelated_count" -gt "$threshold" ]; then
      echo "Found ${pass_total} gate:product-pass card.comment events but only"
      echo "${evidence_correlated} have correlated probe.evidence within ±60 lines."
      echo "${uncorrelated_count} uncorrelated (threshold for fail: >${threshold})."
      echo ""
      echo "Uncorrelated (sample, up to 10):"
      for line in "${uncorrelated[@]:0:10}"; do
        echo "  $line"
      done
      echo ""
      echo "  Today's paper-trail pattern (Jeff 2026-04-30): every"
      echo "  /gate-product PASS must emit probe.evidence with the live-"
      echo "  probe stdout/artifact, not just a card comment. Without the"
      echo "  emission, the PASS is ceremony — caught Wren's PASS on #2625"
      echo "  before production team-block."
      false
    fi
  fi
}
