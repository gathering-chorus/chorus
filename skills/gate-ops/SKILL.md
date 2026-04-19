---
name: gate-ops
description: Operations gate — verify deploy safety, health checks, log flow, rollback path. Silas only. Fires at deploy, not code-complete.
user-invocable: true
---

# /gate-ops — Operations Gate

Fires at deploy time, NOT code-complete. Verifies the change can be run safely. **Silas only.**

## Arguments

```
/gate-ops <card-id>
```

## Owner Check

Only Silas can run this gate. If another role invokes it:
```
Ops gate is owned by Silas.
```
Exit — no checks run.

## Prerequisite

/gate-arch must have passed for this card. Check for `gate:arch-pass` card comment. If not found:
```
WARN: No gate:arch-pass found for this card. Run /gate-arch <card-id> first.
```
In pilot mode: warn but continue. In enforce mode (future): block.

## Applicability Check

Read the card with `cards view <card-id>`. Check the card type label.

- Doc-only / board-process / skill-only cards with no deploy: **SKIP** — "Ops gate not applicable."
- Cards that touch services, scripts, hooks, LaunchAgents, deploy paths: **RUN**.

If skipped, emit: `gate.ops.skipped` spine event. Exit.

## Automated Checks (run all, collect results)

### 1. Service health

```bash
# Check all registered service endpoints respond.
# Includes the Chorus product surface (#2099): landing + /borg/* nine pages.
ENDPOINTS=(
  "http://localhost:3000|caddy-edge"
  "http://localhost:2019/config/|caddy-admin"
  "http://localhost:3002|gathering-app"
  "http://localhost:3030/\$/ping|fuseki"
  "http://localhost:3340/api/athena/health|chorus-api"
  "http://localhost:3340/|chorus-landing"
  "http://localhost:3340/borg/|borg-landing"
  "http://localhost:3340/borg/assessment|borg-assessment"
  "http://localhost:3340/borg/instance-explorer/|borg-instance-explorer"
  "http://localhost:3340/borg/patterns/|borg-patterns"
  "http://localhost:3340/borg/jeff/|borg-jeff"
  "http://localhost:3340/borg/replay/|borg-replay"
  "http://localhost:3340/borg/quality/|borg-quality"
  "http://localhost:3340/borg/fitness/|borg-fitness"
  "http://localhost:3340/borg/cost/|borg-cost"
  "http://localhost:3340/borg/hooks/|borg-hooks"
  "http://localhost:3102/ready|loki"
  "http://localhost:3470/health|bridge"
)
for EP in "${ENDPOINTS[@]}"; do
  URL="${EP%%|*}"
  NAME="${EP##*|}"
  CODE=$(curl -s --max-time 3 -o /dev/null -w '%{http_code}' "$URL" 2>/dev/null)
  [ -z "$CODE" ] && CODE="000"
  if [[ "$CODE" =~ ^(200|204|301|302|308)$ ]]; then
    echo "PASS: $NAME ($CODE)"
  else
    echo "FAIL: $NAME ($CODE)"
  fi
done
```

**Pass:** All endpoints respond with 2xx or 3xx.
**Fail:** Any endpoint unreachable or 4xx/5xx — list the failing services.

### 2. Loki log flow

```bash
# Check if logs are flowing — query Loki for recent entries
NOW=$(date +%s)
START=$(( NOW - 300 ))
RESULT=$(curl -sf --max-time 5 -G "http://localhost:3102/loki/api/v1/query_range" \
  --data-urlencode 'query={job=~"gathering.*"}' \
  --data-urlencode "start=${START}000000000" \
  --data-urlencode "end=${NOW}000000000" \
  --data-urlencode "limit=1" 2>/dev/null)
COUNT=$(echo "$RESULT" | python3 -c "
import sys,json
d=json.load(sys.stdin)
print(sum(len(r.get('values',[])) for r in d.get('data',{}).get('result',[])))
" 2>/dev/null || echo "0")
if [[ "$COUNT" -gt 0 ]]; then
  echo "PASS: logs flowing ($COUNT entries in last 5min)"
else
  echo "WARN: no logs in Loki for last 5min — check log pipeline"
fi
```

**Pass:** Recent log entries exist.
**Warn:** No entries — advisory, not blocking (cadence gaps possible).

### 3. Rollback path

```bash
# Verify the previous commit exists and is reachable
cd /Users/jeffbridwell/CascadeProjects/chorus
PREV=$(git log --oneline -2 | tail -1 | awk '{print $1}')
if [ -n "$PREV" ]; then
  echo "PASS: rollback target $PREV"
else
  echo "FAIL: no prior commit for rollback"
fi
```

**Pass:** Prior commit exists (can `git revert` if needed).
**Fail:** No prior commit.

### 4. Disk health

```bash
# Check disk usage — DEC-022 thresholds
USAGE=$(df -h /System/Volumes/Data 2>/dev/null | tail -1 | awk '{print $5}' | tr -d '%')
if [ "$USAGE" -lt 90 ]; then
  echo "PASS: disk at ${USAGE}%"
elif [ "$USAGE" -lt 95 ]; then
  echo "WARN: disk at ${USAGE}% — warning threshold"
else
  echo "FAIL: disk at ${USAGE}% — critical, blocks deploy"
fi
```

**Pass:** Under 90%.
**Warn:** 90-95%.
**Fail:** Over 95% — critical, blocks deploy.

## No Manual Confirms

All ops checks are automated. No manual step.

## Result

Print summary:

```
## /gate-ops #<card-id>

  Service health:   PASS | FAIL (services listed)
  Loki log flow:    PASS | WARN
  Rollback path:    PASS | FAIL
  Disk health:      PASS | WARN | FAIL (usage%)

  VERDICT: PASS | FAIL
```

## On Pass

1. Emit spine event: `gate.ops.passed` with card ID
2. Add card comment: "gate:ops-pass — Silas"
3. Prompt: "Chain complete. Next: whoever is running /demo or /acp will see all 5 gate passes on the card and proceed." (#2222 — demo-caller is already watching the card; no nudge needed)

## On Fail

1. Emit spine event: `gate.ops.failed` with card ID and failing items
2. Print failing items. Do not nudge forward. Fix the ops issue first.
