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

## Applicability Check

Read the card with `cards view <card-id>`. Check the card type label.

- `type:fix` (bug fix): **SKIP** unless it touches LaunchAgents or deploy paths.
- Doc-only / board-process cards: **SKIP**.
- Feature, infra, script/hook cards that touch LaunchAgents or deploy: **RUN**.

If skipped, emit: `gate.ops.skipped` spine event. Exit.

## Automated Checks (run all, collect results)

### 1. Service health

```bash
# Check app-state status for all services
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/app-state.sh status 2>/dev/null
# Exit 0 = all healthy, non-zero = something down
```

**Pass:** All services report healthy.
**Fail:** List unhealthy services.

### 2. Health endpoint

```bash
# Hit the app health endpoint
curl -sf http://localhost:3000/health --connect-timeout 5 > /dev/null
```

**Pass:** 200 response.
**Fail:** Non-200 or timeout.

### 3. Loki log flow

```bash
# Check if logs are flowing — query Loki for recent entries
curl -s "http://localhost:3102/loki/api/v1/query?query=%7Bjob%3D%22gathering-app%22%7D&limit=1" --connect-timeout 5 | python3 -c "
import json, sys
d = json.load(sys.stdin)
results = d.get('data', {}).get('result', [])
if results:
    print('PASS: logs flowing')
    sys.exit(0)
else:
    print('FAIL: no recent log entries')
    sys.exit(1)
" 2>/dev/null
```

**Pass:** Recent log entries exist.
**Fail:** No entries or Loki unreachable.

### 4. Rollback path

```bash
# Verify the previous commit exists and is reachable
cd /Users/jeffbridwell/CascadeProjects/chorus
git log --oneline -2 | tail -1
# If we can see the prior commit, rollback is possible
```

**Pass:** Prior commit exists (can `git revert` if needed).
**Fail:** No prior commit (shouldn't happen, but check).

## No Manual Confirms

All ops checks are automated. No manual step.

## Result

Print summary:

```
## /gate-ops #<card-id>

  Service health:   PASS | FAIL (details)
  Health endpoint:  PASS | FAIL (status)
  Loki log flow:    PASS | FAIL
  Rollback path:    PASS | FAIL

  VERDICT: PASS | FAIL
```

## On Pass

1. Emit spine event: `gate.ops.passed` with card ID
2. Add card comment: "gate:ops-pass — Silas"
3. Check if all prior gates passed. If yes: nudge Jeff "Pipeline complete on #<card-id> — all gates passed, ready for /acp"
4. If prior gates not all passed: nudge the owner of the next incomplete gate.

## On Fail

1. Emit spine event: `gate.ops.failed` with card ID and failing items
2. Print failing items. Do not nudge forward. Fix the ops issue first.
