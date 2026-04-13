#!/bin/bash
# chorus-log-coverage.sh — verify all log files are routed to Loki (#1984)
# Enumerates .log files across all known paths, checks Loki for each.

set -euo pipefail

LOKI="http://localhost:3102"
NOW=$(date +%s)
START=$(( NOW - 3600 ))
PASS=0
FAIL=0

check_loki() {
  local file="$1"
  local base=$(basename "$file")
  local count=$(curl -sf -G "${LOKI}/loki/api/v1/query_range" \
    --data-urlencode "query={filename=~\".*${base}\"}" \
    --data-urlencode "start=${START}000000000" \
    --data-urlencode "end=${NOW}000000000" \
    --data-urlencode "limit=1" 2>/dev/null | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('data',{}).get('result',[])))" 2>/dev/null || echo 0)
  if [ "$count" -gt 0 ]; then
    echo "$file | in_loki=Y"
    PASS=$((PASS + 1))
  else
    echo "$file | in_loki=N"
    FAIL=$((FAIL + 1))
  fi
}

DIRS=(
  "$HOME/Library/Logs/Chorus"
  "$HOME/Library/Logs/Gathering"
  "$HOME/.chorus"
  "/Users/jeffbridwell/CascadeProjects/chorus/platform/logs"
  "/Users/jeffbridwell/CascadeProjects/chorus/platform/pulse/logs"
  "/Users/jeffbridwell/CascadeProjects/chorus/proving/logs"
)

for dir in "${DIRS[@]}"; do
  if [ -d "$dir" ]; then
    for f in "$dir"/*.log; do
      [ -f "$f" ] && check_loki "$f"
    done
  fi
done

echo "---"
echo "Coverage: ${PASS} in Loki, ${FAIL} missing"
[ "$FAIL" -gt 0 ] && exit 1 || exit 0
