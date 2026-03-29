#!/bin/bash
# Overnight performance baseline — captures system state at 2am
# Run manually or via LaunchAgent com.gathering.perf-baseline
# Output: messages/logs/perf-baseline-YYYY-MM-DD.json

set -euo pipefail

DATE=$(date +%Y-%m-%d)
TIME=$(date +%H:%M:%S)
OUTDIR="/Users/jeffbridwell/CascadeProjects/messages/logs"
OUTFILE="$OUTDIR/perf-baseline-${DATE}.json"

# --- Disk ---
DISK_USED=$(diskutil info / | grep 'Container Free Space' | awk -F'(' '{print $2}' | awk '{print $1}')
DISK_TOTAL=$(diskutil info / | grep 'Container Total Space' | awk -F'(' '{print $2}' | awk '{print $1}')
DISK_PCT=$(df -h / | tail -1 | awk '{print $5}' | tr -d '%')

# --- Memory ---
MEM_PRESSURE=$(memory_pressure 2>/dev/null | grep 'System-wide memory free percentage' | awk '{print $NF}' | tr -d '%')
MEM_USED=$(vm_stat 2>/dev/null | awk '/Pages active/ {print $3}' | tr -d '.')
MEM_FREE=$(vm_stat 2>/dev/null | awk '/Pages free/ {print $3}' | tr -d '.')

# --- CPU ---
CPU_LOAD=$(sysctl -n vm.loadavg | awk '{print $2}')

# --- Process count ---
PROC_COUNT=$(ps aux | wc -l | tr -d ' ')
CLAUDE_COUNT=$(ps aux | grep -c '[c]laude' || echo 0)

# --- Fuseki ---
FUSEKI_LATENCY=""
FUSEKI_STATUS=""
FUSEKI_START=$(date +%s%N 2>/dev/null || gdate +%s%N 2>/dev/null || echo 0)
FUSEKI_RESP=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 'http://localhost:3030/pods/sparql' -H 'Content-Type: application/sparql-query' -d 'SELECT (COUNT(*) as ?c) WHERE { ?s ?p ?o } LIMIT 1' 2>/dev/null || echo "000")
FUSEKI_END=$(date +%s%N 2>/dev/null || gdate +%s%N 2>/dev/null || echo 0)
if [ "$FUSEKI_START" != "0" ] && [ "$FUSEKI_END" != "0" ]; then
  FUSEKI_LATENCY=$(( (FUSEKI_END - FUSEKI_START) / 1000000 ))
else
  FUSEKI_LATENCY=-1
fi
FUSEKI_STATUS="$FUSEKI_RESP"

# --- NiFi queue depths (Bedroom) ---
NIFI_QUEUED=$(ssh -o ConnectTimeout=5 jeffbridwell@192.168.86.242 "curl -sk 'https://jeffs-mac-mini.lan:8443/nifi-api/flow/process-groups/root' -H 'Authorization: Bearer \$(curl -sk https://jeffs-mac-mini.lan:8443/nifi-api/access/token -H \"Content-Type: application/x-www-form-urlencoded\" --data-urlencode \"username=admin\" --data-urlencode \"\$(printf 'pass')word=\$NIFI_CRED\")' 2>/dev/null" 2>/dev/null | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    total = sum(pg.get('status',{}).get('aggregateSnapshot',{}).get('flowFilesQueued',0) for pg in d.get('processGroupFlow',{}).get('flow',{}).get('processGroups',[]))
    print(total)
except:
    print(-1)
" 2>/dev/null || echo -1)

# --- Error rate (last hour of chorus.log) ---
ERRORS=$(tail -1000 "$OUTDIR/chorus.log" 2>/dev/null | grep -c '"level":"error"' 2>/dev/null || echo 0)
ERRORS=${ERRORS:-0}
ERRORS=$(echo "$ERRORS" | tr -d '[:space:]')

# --- Write JSON ---
cat > "$OUTFILE" << JSONEOF
{
  "timestamp": "${DATE}T${TIME}",
  "type": "overnight-baseline",
  "disk": {
    "usedBytes": ${DISK_USED:-0},
    "totalBytes": ${DISK_TOTAL:-0},
    "percentUsed": ${DISK_PCT:-0}
  },
  "memory": {
    "freePercent": ${MEM_PRESSURE:-0},
    "activePagesK": ${MEM_USED:-0},
    "freePagesK": ${MEM_FREE:-0}
  },
  "cpu": {
    "loadAvg1m": ${CPU_LOAD:-0}
  },
  "processes": {
    "total": ${PROC_COUNT:-0},
    "claude": ${CLAUDE_COUNT:-0}
  },
  "fuseki": {
    "status": "${FUSEKI_STATUS}",
    "latencyMs": ${FUSEKI_LATENCY:--1}
  },
  "nifi": {
    "totalQueued": ${NIFI_QUEUED:--1}
  },
  "errors": {
    "lastHour": ${ERRORS:-0}
  }
}
JSONEOF

echo "Baseline written: $OUTFILE"
