#!/usr/bin/env bash
# startup-sync-failure.check.sh — #3709: the startup-sync-failure check, EXTRACTED from its yml.
#
# Why a script and not an inline block: alert-runner.sh extracts an inline
# check with `awk '/^check: \|/{f=1;next} /^[a-z]/{if(f)exit} f{print}'`, which
# stops at the first column-0 lowercase line. Embedded python (`import ...`,
# `try:`) sits at column 0, so the runner silently cut this script in half and
# ran something that did not parse — the check could never report CI's state,
# only its own early-exit. Same move #2327 made for fuseki-harvest.
set -uo pipefail

# Step 1: Is Fuseki healthy? If yes, no alert needed — sync failures
# without Fuseki being down are credential issues (Twilio 401), not data loss.
FUSEKI_OK=$(curl -sf --max-time 3 -o /dev/null -w '%{http_code}' \
  "http://localhost:3030/$/ping" 2>/dev/null || echo "000")
if [[ "$FUSEKI_OK" == "200" ]]; then
  echo "ok"
  exit 0
fi

# Step 2: Fuseki is down. Check if there was also a sync failure recently.
NOW=$(date +%s)
START=$(( NOW - 300 ))
RESULT=$(curl -sf --max-time 5 -G "http://localhost:3102/loki/api/v1/query_range" \
  --data-urlencode 'query={job=~"gathering.*"} |~ "sync failed"' \
  --data-urlencode "start=${START}000000000" \
  --data-urlencode "end=${NOW}000000000" \
  --data-urlencode "limit=3" 2>/dev/null)
COUNT=$(echo "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(sum(len(r.get('values',[])) for r in d.get('data',{}).get('result',[])))" 2>/dev/null || echo "0")
if [[ "$COUNT" == "0" ]]; then
  echo "ok — fuseki down but no sync failures"
  exit 0
fi

# Extract actual error message from logs
ERROR_MSG=$(echo "$RESULT" | python3 -c "
import sys,json
d=json.load(sys.stdin)
for r in d.get('data',{}).get('result',[]):
for ts,msg in r.get('values',[]):
  try:
    j=json.loads(msg)
    print(j.get('error',j.get('message','unknown error'))[:150])
  except: print(msg[:150])
  sys.exit(0)
print('unknown')
" 2>/dev/null || echo "unknown")
echo "failed:fuseki_down+sync_error:$ERROR_MSG"
exit 1

