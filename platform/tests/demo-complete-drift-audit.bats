#!/usr/bin/env bats
# @test-type: unit — static source/shape guard, hermetic
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
# #4131 — audit the SPINE, not the repo stub. platform/logs/chorus.log under
# canonical is a fresh CI-created file with no demo or accept history, so every
# case skipped and the suite read UNMEASURED ("no parseable output") in the
# nightly. The events this audits are written to ~/.chorus/chorus.log; this
# test only reads it. CHORUS_SPINE stays the seam for a fixture spine.
CHORUS_LOG="${CHORUS_SPINE:-$HOME/.chorus/chorus.log}"

setup() {
  if [ ! -f "$CHORUS_LOG" ]; then
    echo "spine missing at $CHORUS_LOG — the audit has nothing to read; that is a defect, not a skip (#4131)"; false
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

  # #4131 — ONE pass over the spine. The spine is 2 GB and never rotates
  # (Jeff's ruling); the old loop re-read it three times per started demo,
  # so the unit died at its cap with no TAP line and the nightly scored it
  # UNMEASURED (12:12 run). Index every line this audit can need, with its
  # line number, once; every lookup below reads the index. And audit the
  # NEWEST 20 demos (tail), not the oldest 20 in the file (head).
  SPINE_IDX=$(mktemp)
  grep -nE "\"event\":\"(card\.demo\.started|demo\.complete|card\.accepted|card\.rejected)\"" "$CHORUS_LOG" 2>/dev/null > "$SPINE_IDX" || true
  # A demo presented and still waiting for Jeff's go is OPEN, not drifted:
  # the first indexed run found 6 "uncorrelated" and all 6 were today's
  # presented-not-yet-accepted demos (#4125, #4131). Audit demos older than
  # DEMO_OPEN_HOURS (default 24). And /demo writes card.demo.started twice
  # per demo, ~30ms apart; count a demo once (card + second).
  open_cutoff=$(date -u -v-${DEMO_OPEN_HOURS:-24}H +"%Y-%m-%dT%H:%M:%S" 2>/dev/null \
    || date -u -d "${DEMO_OPEN_HOURS:-24} hours ago" +"%Y-%m-%dT%H:%M:%S")
  started_lines=$(grep "\"event\":\"card\.demo\.started\"" "$SPINE_IDX" \
    | awk -v cut="$open_cutoff" '{
        if (match($0, /"timestamp":"[^"]+"/)) { ts = substr($0, RSTART+13, RLENGTH-14) } else { next }
        utc = ts; sub(/[.][0-9]+/, "", utc); sub(/[+-][0-9][0-9]:?[0-9][0-9]$|Z$/, "", utc)
        if (utc > cut) next
        if (match($0, /"card(_id)?":"?[0-9]+/)) { c = substr($0, RSTART, RLENGTH); gsub(/[^0-9]/, "", c) } else { next }
        k = c ":" substr(ts, 1, 19); if (seen[k]++) next
        print }' \
    | tail -20)

  if [ -z "$started_lines" ]; then
    echo "no card.demo.started events in the window — nothing drifted (#4131: an empty window is a pass, not a skip)"; return 0
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

    start_lineno="${start_line%%:*}"

    if [ -z "$start_lineno" ]; then
      continue
    fi

    # #4131 — look forward for the rest of the spine, not 500 lines. The spine
    # writes thousands of lines a minute now, so 500 lines is seconds, and a
    # demo's terminal state (Jeff's go, then the land's card.accepted) lands
    # 20-40 minutes after card.demo.started: every started demo read as
    # unclosed the moment the audit could read a real spine.
    terminal=$(awk -F: -v n="$start_lineno" '$1 > n' "$SPINE_IDX" \
      | grep -E "\"event\":\"(demo\.complete|card\.accepted|card\.rejected)\"" \
      | grep -E "\"card[_id]*\":\"?${start_card}\"?" \
      | head -1)

    if [ -n "$terminal" ]; then
      closed=$((closed + 1))
    else
      uncorrelated+=("#${start_card} @ ${start_ts}")
    fi
  done <<< "$started_lines"
  rm -f "$SPINE_IDX"

  # Threshold: absolute-count >3 (per Kade preview-feedback, same Why-3
  # rationale as spine-emit-drift-audit.bats — see that file's comment
  # block for the absolute-vs-ratio reasoning).
  if [ "$total" -gt 0 ]; then
    uncorrelated_count=$((total - closed))
    if [ "$uncorrelated_count" -gt 3 ]; then
      echo "Found ${total} card.demo.started events but only ${closed} closed."
      echo "${uncorrelated_count} uncorrelated (threshold for fail: >3 absolute)."
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
