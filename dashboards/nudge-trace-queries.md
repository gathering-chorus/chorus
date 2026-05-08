# Nudge Trace Queries (#2765 AC5)

Six named Loki queries from the [nudge-service-design](../designing/docs/nudge-service-design.html) Observability section. Each is part of the contract — they're how the team answers questions about nudge behavior. Validated against live nudge stream as part of #2765 acceptance.

The correlation primitive is `trace_id` — UUIDv7 minted at sender, propagated via `X-Chorus-Trace-Id` HTTP header, written as a column on `messages.db.messages`, and stamped on every spine event in the lifecycle (`nudge.requested`, `nudge.surfaced`, `nudge.surface.failed`, `nudge.health.*`).

## 1. All TCC-denied failures

Signals signing churn or revoked Accessibility grant. Should be near-zero with stable keychain-identity signing (#2548).

```logql
{event="nudge.surface.failed"} | json | reason="tcc-denied"
```

## 2. Full chronological story of one nudge

Replace `<id>` with the trace_id from a messages.db row. Returns sender mint → request → attempt(s) → surfaced/failed in order.

```logql
{event=~"nudge\\..*"} | json | trace_id="<id>"
```

Pair with the SQL side for join verification:

```sql
SELECT trace_id, "from", "to", content, delivery_status, delivered_at, last_delivery_error
FROM messages WHERE trace_id = '<id>';
```

## 3. p95 emitted-to-surfaced latency, last hour

The osascript step is the floor (~0.5–1.5s); queue lag is what this measures.

```logql
quantile_over_time(0.95, nudge_latency_ms[1h])
```

## 4. Failure breakdown by typed reason, last 24h

Drives the alerting buckets — TCC-denied vs no-window-found vs transient.

```logql
sum by (reason) (count_over_time({event="nudge.surface.failed"}[24h]))
```

## 5. Nudges that took more than 3 attempts

Retry storms or transient-class confusion (something the worker classifies as transient that actually has a permanent root cause).

```logql
{event="nudge.surface.failed"} | json | attempt > 3
```

## 6. Stuck-nudge probe (synthetic)

Emitted with no terminal event (`nudge.surfaced` or `nudge.surface.failed permanent=true` or final `permanent=false` after exhaustion) after 30s. Detects delivery silently dropping. Implementation is in #2748 (synthetic self-to-self nudge every N minutes; alert when 24h delivery rate < 99.9%).

```logql
# Pseudo-query — actual probe lives in #2748
sum(count_over_time({event="nudge.requested"}[30s]))
  - sum(count_over_time({event=~"nudge\\.(surfaced|surface\\.failed)"}[30s]))
  > 0
```

## Verification (AC5 + AC6)

For acceptance, run all five concrete queries (1–5) against live Loki and confirm:
- Non-empty results for queries on a recent live nudge stream
- All bindings parse as expected fields (trace_id, reason, attempt, etc.)
- Query 2's trace_id, taken from a recent messages.db row via `SELECT trace_id FROM messages WHERE id = (SELECT MAX(id) FROM messages WHERE type='nudge')`, returns the full lifecycle for that nudge in chronological order

Per AC6: spine ↔ messages.db join verified — pick any messages row, query its trace_id in chorus.log, get the full chronological event story.

## Field schema (per design doc Observability section)

Required fields per nudge event, enforced by `chorus_log` helper at the call site (#2765 AC4):

| Event | Required fields |
|-------|----------------|
| `nudge.requested` | `trace_id`, `sender_role`, `receiver_role`, `origin` |
| `nudge.surfaced` | `trace_id`, `sender_role`, `receiver_role`, `attempt`, `cdhash`, `latency_ms` |
| `nudge.surface.failed` | `trace_id`, `sender_role`, `receiver_role`, `attempt`, `reason`, `permanent` |
| `nudge.health.*` | `trace_id` (when applicable), `condition`, `detail` |
