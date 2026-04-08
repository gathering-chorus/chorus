# Brief: #1051 Kill 5 T3 Metric Paths (Phase 3 of #1040)

**From:** Wren | **To:** Kade | **Card:** #1051 | **Priority:** P2
**Depends on:** #1050 (Phase 2, Silas) for Won't Do spine differentiation — but T3 kills 1-2 can proceed now

## Context

DEC-070: every metric traces to a structured spine event. Five computation hacks in `loom-metrics.sh` use git log grep or remainder math instead of querying structured data. Full metric catalog: `messages/schemas/metrics-manifest.json`.

## T3 Kill List

### Kill 1: werk.ops (lines 105-108) — Easy, no dependency
**Current:** `ops = len(open_tasks) - known_open` — remainder math breaks when bucket structure changes.
**Replace:** Parse ALL bucket names from `board-ts buckets` output. Standard buckets = {Now, Next, Later, WIP, Harvesting, Ideas, Done, Won't Do}. Sum counts for non-standard buckets. This is explicit, not remainder.

### Kill 2: werk.wont_do (lines 110-121) — Easy, no dependency
**Current:** Label scan loop on done tasks looking for 'wontdo' label, with fallback to bucket count.
**Replace:** `bucket_counts.get("Won't Do", 0)` — one line. The bucket count IS ground truth. Delete the 12-line label scan.

### Kill 3: weekly_throughput (lines 142-174) — Loki query
**Current:** Git commit message grep for 'done/shipped/session close/accepted' keywords + card # extraction.
**Replace:** Loki query for `card.accepted` events:
```
{app="chorus-events"} |= `"event":"card.accepted"` | json
```
Group by ISO week from timestamp. Dedup by card_id. Return last 6 weeks.

### Kill 4: operations.sessions (lines 348-358) — Loki query
**Current:** Git commit grep for 'session close/reboot/start'.
**Replace:** Loki query for `session.role.started` events:
```
{app="chorus-events"} |= `"event":"session.role.started"` | json
```
Count events in date range. No dedup needed.

### Kill 5: operations.deploys (lines 360-374) — Loki query, BUG FIX
**Current:** Git grep for 'deploy' PLUS chorus.log file grep for deploy.pipeline.completed — **double-counts** when both sources record the same deploy.
**Replace:** Loki query for `deploy.pipeline.completed` ONLY:
```
{app="chorus-events"} |= `"event":"deploy.pipeline.completed"` | json
```
Single source. No double-count.

## query_loki() Helper

Add a reusable function for all 3 Loki queries:

```bash
# Loki endpoint: http://localhost:3102/loki/api/v1/query_range
# Timeout: 5 seconds
# On failure: return empty result (not crash)
# Date range: use $METRICS_SINCE / $METRICS_UNTIL env vars
query_loki() {
  local logql="$1"
  local start="${METRICS_SINCE:-$(date -v-30d +%Y-%m-%dT00:00:00Z)}"
  local end="${METRICS_UNTIL:-$(date +%Y-%m-%dT23:59:59Z)}"
  curl -s --max-time 5 \
    "http://localhost:3102/loki/api/v1/query_range" \
    --data-urlencode "query=$logql" \
    --data-urlencode "start=$start" \
    --data-urlencode "end=$end" \
    --data-urlencode "limit=5000" \
    2>/dev/null || echo '{"data":{"result":[]}}'
}
```

## Final Verification

After all 5 kills:
1. `grep -c 'git.*log' loom-metrics.sh` returns 0
2. `data_window` output includes `t3_paths_remaining: 0`
3. Loom and Werk Instruments show identical numbers for shared metrics (accepted, rejected, deploys) when viewing the same date range

## Historical Gap

Spine events start ~2026-03-01. Pre-March throughput/sessions/deploys will show 0 from Loki. This is accepted (DEC-070). The `data_window` note communicates this honestly to the UI.

## Deploy

Only needed if `team.handler.ts` changes (spine coverage in response). The shell script changes are immediate — loom-metrics.sh is invoked on request, not containerized.
