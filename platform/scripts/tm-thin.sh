#!/bin/bash
# tm-thin.sh — Delete Time Machine local snapshots older than 24 hours
# Prevents APFS container bloat on heavy-write sessions.
# Scheduled daily via LaunchAgent com.chorus.tm-thin.

set -euo pipefail

KEEP_HOURS=24
NOW=$(date +%s)
DELETED=0

for snap in $(tmutil listlocalsnapshots / 2>/dev/null | grep "com.apple.TimeMachine"); do
    # Extract date: com.apple.TimeMachine.2026-03-31-092030.local
    date_str=$(echo "$snap" | sed 's/com.apple.TimeMachine.//' | sed 's/.local//')
    # Parse: 2026-03-31-092030 → 2026-03-31 09:20:30
    formatted=$(echo "$date_str" | sed 's/\([0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\}\)-\([0-9]\{2\}\)\([0-9]\{2\}\)\([0-9]\{2\}\)/\1 \2:\3:\4/')
    snap_epoch=$(date -j -f "%Y-%m-%d %H:%M:%S" "$formatted" +%s 2>/dev/null || echo 0)

    if [ "$snap_epoch" -gt 0 ]; then
        age_hours=$(( (NOW - snap_epoch) / 3600 ))
        if [ "$age_hours" -gt "$KEEP_HOURS" ]; then
            tmutil deletelocalsnapshots "$date_str" 2>/dev/null && ((DELETED++)) || true
        fi
    fi
done

echo "tm-thin: deleted $DELETED snapshots older than ${KEEP_HOURS}h"
