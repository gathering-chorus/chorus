#!/bin/bash
# daily-review-summary.sh — 6am aggregated review, posts to Bridge
# Card #1766 | Runs ops + quality, combines into one post
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/bridge-post.sh"

BRIDGE="http://localhost:3470/api/message"
CHORUS_LOG="$SCRIPT_DIR/chorus-log"
TIMESTAMP=$(TZ=America/New_York date '+%Y-%m-%d %H:%M')

# Run both reviews and capture output
OPS_OUTPUT=$(bash "$SCRIPT_DIR/daily-review-ops.sh" 2>/dev/null || echo "⚠ Ops review failed")
QUALITY_OUTPUT=$(bash "$SCRIPT_DIR/daily-review-quality.sh" 2>/dev/null || echo "⚠ Quality review failed")

# Determine overall status from component outputs
OVERALL="🟢"
echo "$OPS_OUTPUT$QUALITY_OUTPUT" | grep -q "🔴" && OVERALL="🔴"
echo "$OPS_OUTPUT$QUALITY_OUTPUT" | grep -q "🟡" && [ "$OVERALL" != "🔴" ] && OVERALL="🟡"

# Board snapshot
BOARD_DIR="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts"
WIP_COUNT=$(bash "$BOARD_DIR/cards" mine all 2>/dev/null | grep -c "WIP" || true)
WIP_COUNT=${WIP_COUNT:-0}
DONE_TODAY=$(bash "$BOARD_DIR/cards" list Done 2>/dev/null | grep "$(date +%Y-%m-%d)" | wc -l | tr -d ' ' || true)
DONE_TODAY=${DONE_TODAY:-0}

BODY="$OVERALL **Daily Review** — $TIMESTAMP

---
$OPS_OUTPUT

---
$QUALITY_OUTPUT

---
**Board:** ${WIP_COUNT} WIP, ${DONE_TODAY} completed today."

# Post combined summary to Bridge (with retry)
bridge_post "$BRIDGE" "wren" "$BODY" || true

# Determine overall status string for spine event
OVERALL_STATUS="green"
echo "$OPS_OUTPUT$QUALITY_OUTPUT" | grep -q "🔴" && OVERALL_STATUS="red"
echo "$OPS_OUTPUT$QUALITY_OUTPUT" | grep -q "🟡" && [ "$OVERALL_STATUS" != "red" ] && OVERALL_STATUS="yellow"

# Emit completion event
"$CHORUS_LOG" daily.review.completed silas status=$OVERALL_STATUS wip=$WIP_COUNT done_today=$DONE_TODAY 2>/dev/null || true

echo "$BODY"
