#!/bin/bash
# correlation-timeline.sh — Merged event timeline across services (#2280)
#
# Queries spine events, hook decisions, Alertmanager alerts, and git commits
# within a time window and merges them into a sorted, human-readable timeline.
#
# Usage:
#   correlation-timeline.sh --from "2026-04-06 14:00" --to "2026-04-06 14:30"
#   correlation-timeline.sh --last 1h
#   correlation-timeline.sh --last 30m

set -euo pipefail

CHORUS_LOG="$HOME/Library/Logs/Chorus/chorus.log"
HOOKS_LOG="$HOME/Library/Logs/Gathering/hooks.log"
ALERTMANAGER_URL="http://localhost:9093"
REPO_DIR="$(cd "$(dirname "$0")/../.." && pwd)"

FROM=""
TO=""
LAST=""

show_help() {
    cat <<'EOF'
correlation-timeline.sh — Merged event timeline across services

Usage:
  correlation-timeline.sh --from "2026-04-06 14:00" --to "2026-04-06 14:30"
  correlation-timeline.sh --last 1h
  correlation-timeline.sh --last 30m

Options:
  --from <datetime>   Start of window (YYYY-MM-DD HH:MM)
  --to <datetime>     End of window (YYYY-MM-DD HH:MM)
  --last <duration>   Relative window: 30m, 1h, 2h, 1d
  --help              Show this help

Sources:
  [spine]   Chorus spine events (chorus.log)
  [hooks]   Hook decisions (hooks.log)
  [alerts]  Alertmanager alert history
  [git]     Git commits across repos

Output: one line per event, sorted by timestamp.
  2026-04-06T14:05:23 [spine]  card.pulled | silas card=2285
  2026-04-06T14:05:30 [hooks]  pre_tool_use | Edit | silas | allow
  2026-04-06T14:06:00 [alerts] FIRING EndpointDown — http://192.168.86.242:4533/
  2026-04-06T14:07:12 [git]    silas: fix blast radius gate
EOF
}

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --from) FROM="$2"; shift 2 ;;
        --to) TO="$2"; shift 2 ;;
        --last) LAST="$2"; shift 2 ;;
        --help|-h) show_help; exit 0 ;;
        *) echo "Unknown arg: $1" >&2; show_help; exit 1 ;;
    esac
done

# Resolve time window
if [ -n "$LAST" ]; then
    case "$LAST" in
        *m) SECONDS_AGO=$(( ${LAST%m} * 60 )) ;;
        *h) SECONDS_AGO=$(( ${LAST%h} * 3600 )) ;;
        *d) SECONDS_AGO=$(( ${LAST%d} * 86400 )) ;;
        *)  echo "Invalid duration: $LAST (use 30m, 1h, 2h, 1d)" >&2; exit 1 ;;
    esac
    TO_EPOCH=$(date +%s)
    FROM_EPOCH=$((TO_EPOCH - SECONDS_AGO))
    FROM=$(TZ=America/New_York date -r "$FROM_EPOCH" '+%Y-%m-%d %H:%M')
    TO=$(TZ=America/New_York date -r "$TO_EPOCH" '+%Y-%m-%d %H:%M')
elif [ -z "$FROM" ] || [ -z "$TO" ]; then
    echo "Provide --from/--to or --last" >&2
    show_help
    exit 1
fi

# Convert to comparable formats
FROM_ISO=$(TZ=America/New_York date -j -f '%Y-%m-%d %H:%M' "$FROM" '+%Y-%m-%dT%H:%M:00' 2>/dev/null || echo "$FROM")
TO_ISO=$(TZ=America/New_York date -j -f '%Y-%m-%d %H:%M' "$TO" '+%Y-%m-%dT%H:%M:59' 2>/dev/null || echo "$TO")

TMPFILE=$(mktemp)
trap 'rm -f "$TMPFILE"' EXIT

# --- Source 1: Spine events (chorus.log) ---
if [ -f "$CHORUS_LOG" ]; then
    python3 - "$CHORUS_LOG" "$FROM_ISO" "$TO_ISO" << 'PYEOF' >> "$TMPFILE"
import json, sys
log_file, from_iso, to_iso = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    with open(log_file) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                ev = json.loads(line)
                ts = ev.get("timestamp", "")
                if not ts or ts[:19] < from_iso[:19] or ts[:19] > to_iso[:19]:
                    continue
                event = ev.get("event", "?")
                role = ev.get("role", "?")
                skip = {"appName", "timestamp", "event", "role", "level", "digest"}
                detail = " ".join(f"{k}={v}" for k, v in ev.items() if k not in skip and v)
                ts_short = ts[:19]
                print(f"{ts_short} [spine]  {event} | {role} {detail}")
            except (json.JSONDecodeError, KeyError):
                continue
except FileNotFoundError:
    pass
PYEOF
fi

# --- Source 2: Hook decisions (hooks.log) ---
if [ -f "$HOOKS_LOG" ]; then
    python3 - "$HOOKS_LOG" "$FROM_ISO" "$TO_ISO" << 'PYEOF' >> "$TMPFILE"
import sys, re
log_file, from_iso, to_iso = sys.argv[1], sys.argv[2], sys.argv[3]
try:
    with open(log_file) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            parts = [p.strip() for p in line.split("|")]
            if len(parts) < 6:
                continue
            ts_raw = parts[0]
            ts_date = ts_raw[:10]
            ts_time = ts_raw[11:19] if len(ts_raw) > 18 else ""
            ts_cmp = f"{ts_date}T{ts_time}"
            if ts_cmp < from_iso[:19] or ts_cmp > to_iso[:19]:
                continue
            hook = parts[1].strip() if len(parts) > 1 else "?"
            tool = parts[2].strip() if len(parts) > 2 else "?"
            role = parts[3].strip() if len(parts) > 3 else "?"
            result = parts[5].strip() if len(parts) > 5 else "?"
            result_lower = result.lower()
            if "deny" in result_lower or "block" in result_lower or "warn" in result_lower:
                print(f"{ts_cmp} [hooks]  {hook} | {tool} | {role} | {result}")
except FileNotFoundError:
    pass
PYEOF
fi

# --- Source 3: Alertmanager alerts ---
alerts_json=$(curl -sf --max-time 5 "${ALERTMANAGER_URL}/api/v2/alerts?active=true&silenced=false&inhibited=false" 2>/dev/null || echo "[]")
if [ "$alerts_json" != "[]" ]; then
    echo "$alerts_json" | python3 - "$FROM_ISO" "$TO_ISO" << 'PYEOF' >> "$TMPFILE"
import json, sys
from_iso, to_iso = sys.argv[1], sys.argv[2]
try:
    alerts = json.load(sys.stdin)
    for a in alerts:
        starts = a.get("startsAt", "")[:19]
        if starts and starts >= from_iso[:19] and starts <= to_iso[:19]:
            name = a.get("labels", {}).get("alertname", "?")
            severity = a.get("labels", {}).get("severity", "?")
            summary = a.get("annotations", {}).get("summary", "")
            state = a.get("status", {}).get("state", "active")
            print(f"{starts} [alerts] {state.upper()} {name} ({severity}) — {summary}")
except (json.JSONDecodeError, KeyError):
    pass
PYEOF
fi

# --- Source 4: Git commits ---
for repo_dir in "$REPO_DIR" "$REPO_DIR/../jeff-bridwell-personal-site" "$REPO_DIR/../shared-observability"; do
    if [ -d "$repo_dir/.git" ]; then
        repo_name=$(basename "$repo_dir")
        git -C "$repo_dir" log --format="%aI %s" --after="$FROM" --before="$TO" 2>/dev/null | while IFS= read -r line; do
            ts=$(echo "$line" | cut -d' ' -f1 | cut -c1-19)
            msg=$(echo "$line" | cut -d' ' -f2-)
            echo "${ts} [git]    ${repo_name}: ${msg}"
        done >> "$TMPFILE"
    fi
done

# --- Merge and sort ---
if [ -s "$TMPFILE" ]; then
    sort "$TMPFILE"
else
    echo "No events found between $FROM and $TO"
fi
