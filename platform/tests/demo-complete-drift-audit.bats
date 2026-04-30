#!/usr/bin/env bats
# demo-complete-drift-audit.bats — #2630 wave 4
#
# Catches the failure pattern Jeff named 2026-04-30: "/demo says step 5
# [feedback] is mandatory — routinely skipped, no detector" and "/demo
# says demo:complete is emitted — almost never run."
#
# Same audit shape as spine-emit-drift-audit.bats — for each
# card.demo.started spine event in window, assert a corresponding
# demo.complete (or terminal demo state) within ±N lines.

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"
CHORUS_LOG="$CHORUS_ROOT/platform/logs/chorus.log"

setup() {
  if [ ! -f "$CHORUS_LOG" ]; then
    skip "chorus.log missing at $CHORUS_LOG"
  fi
}

@test "every recent card.demo.started has a terminal demo state within window" {
  # /demo emits card.demo.started at step 6 (signal). Step 5 [feedback]
  # fires nudges; the close-of-loop is either demo.complete OR
  # card.accepted (Jeff /acp's after demo) OR card.rejected.
  #
  # The failure mode this catches: card.demo.started fired, then NO
  # terminal demo state in the next N lines. Means demo started but
  # never closed — pattern Jeff named.

  started_lines=$(grep "\"event\":\"card\.demo\.started\"" "$CHORUS_LOG" 2>/dev/null \
    | head -20)

  if [ -z "$started_lines" ]; then
    skip "no card.demo.started events in chorus.log — nothing to audit"
  fi

  uncorrelated=()
  total=0
  closed=0

  while IFS= read -r start_line; do
    total=$((total + 1))
    start_card=$(echo "$start_line" | grep -oE "\"card[_id]*\":\"?[0-9]+\"?" \
      | head -1 | grep -oE "[0-9]+" | head -1)
    start_ts=$(echo "$start_line" | grep -oE "\"timestamp\":\"[^\"]+\"" \
      | head -1 | sed -E 's/.*"timestamp":"([^"]+)".*/\1/')

    if [ -z "$start_card" ] || [ -z "$start_ts" ]; then
      continue
    fi

    start_lineno=$(grep -n "\"event\":\"card\.demo\.started\"" "$CHORUS_LOG" \
      | grep "$start_ts" | head -1 | cut -d: -f1)

    if [ -z "$start_lineno" ]; then
      continue
    fi

    # Look forward up to 500 lines for any terminal demo event for this card
    window_end=$((start_lineno + 500))

    terminal=$(sed -n "${start_lineno},${window_end}p" "$CHORUS_LOG" 2>/dev/null \
      | grep -E "\"event\":\"(demo\.complete|card\.accepted|card\.rejected)\"" \
      | grep -E "\"card[_id]*\":\"?${start_card}\"?" \
      | head -1)

    if [ -n "$terminal" ]; then
      closed=$((closed + 1))
    else
      uncorrelated+=("#${start_card} @ ${start_ts}")
    fi
  done <<< "$started_lines"

  # Threshold: fail if more than 50% are uncorrelated (per-subagent finding:
  # all-or-nothing gate misses partial drift).
  if [ "$total" -gt 0 ]; then
    uncorrelated_count=$((total - closed))
    threshold=$((total / 2))
    if [ "$uncorrelated_count" -gt "$threshold" ]; then
      echo "Found ${total} card.demo.started events but only ${closed} closed."
      echo "${uncorrelated_count} uncorrelated (threshold for fail: >${threshold})."
      echo ""
      echo "Uncorrelated (sample, up to 10):"
      for line in "${uncorrelated[@]:0:10}"; do
        echo "  $line"
      done
      echo ""
      echo "  Pattern Jeff named (2026-04-30): /demo skill says step 5"
      echo "  [feedback] is mandatory and demo:complete must emit, but"
      echo "  the steps are routinely skipped by the invoker. Without a"
      echo "  detector, the skill is advisory — this test is the detector."
      false
    fi
  fi
}
