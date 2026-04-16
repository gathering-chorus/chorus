# ADR-024: Common Message Envelope

**Date**: 2026-04-16
**Status**: Accepted
**Decider**: Jeff Bridwell
**Context card**: #2097
**Prior art**: Jeff's Staples ESB message header design (~2008), US Patent 9552400B2

## Context

The seed pipeline broke (RCA #16) and it took 30 minutes to trace across Twilio console, Loki, Fuseki, and cloudflared logs. Each service logs its own way — spine events have `role` and `component`, seeds have their own shape, nudges have theirs. No correlation ID, no hop-level tracing, no common error classification. The system can't trace a message end-to-end or describe its own integrations.

Jeff built exactly this pattern at Staples for the ESB project — a common message header wrapping all payloads. Correlation ID, domain/service/instance metadata, routing key, hop-level tracing. The envelope made the system self-diagnosing and the integration map a byproduct of traffic.

## Decision

Every message in Chorus gets a common envelope. The payload varies, the envelope is standard.

### Envelope Type

```typescript
interface ChorusEnvelope {
  correlationId: string;      // UUID, generated at origin, carried through all hops
  timestamp: string;          // ISO 8601
  source: {
    domain: string;           // e.g., "seeds", "chorus", "photos"
    service: string;          // e.g., "twilio-webhook", "chorus-api", "fuseki"
    instance?: string;        // e.g., "401-592-2496", "localhost:3340"
  };
  destination?: {
    domain: string;
    service: string;
    instance?: string;
  };
  hop: number;                // 1-based, increments at each boundary crossing
  callStack: 'integration' | 'ui' | 'batch' | 'convergence';
  error?: {
    classification: 'transient' | 'permanent' | 'validation';
    message: string;
    retryable: boolean;
  };
  latencyMs?: number;         // time spent at this hop
  parentHop?: number;         // for fan-out: which hop spawned this one
}
```

### Four Call Stacks

| Stack | Pattern | Example |
|-------|---------|---------|
| **integration** | Service → service, real-time | Twilio → app → Fuseki → router → inbox |
| **ui** | Page → API calls | domain-detail.html → 4 API endpoints |
| **batch** | Scheduled pipeline | Crawler → 41 domain snapshots → index |
| **convergence** | Graph population | TTL load → SHACL validate → completeness |

All four use the same envelope. The `callStack` field distinguishes them for filtering.

### Common Error Handling

Every hop classifies errors consistently:
- **transient** — retry with backoff (network timeout, 503, locked resource)
- **permanent** — dead letter with full trace (401 auth, 404 not found, schema violation)
- **validation** — reject to sender (missing field, bad format, constraint violation)

Today's seed failure: Twilio 401 was permanent but classified as transient (warn, retried 17 times). With this model, first 401 → permanent → dead letter → deep-health sees it immediately.

### Trace Storage

SQLite table `traces` in `~/.chorus/index.db`:

```sql
CREATE TABLE traces (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  correlation_id TEXT NOT NULL,
  hop INTEGER NOT NULL,
  call_stack TEXT NOT NULL,
  source_domain TEXT,
  source_service TEXT,
  source_instance TEXT,
  dest_domain TEXT,
  dest_service TEXT,
  dest_instance TEXT,
  timestamp TEXT NOT NULL,
  latency_ms INTEGER,
  error_class TEXT,
  error_message TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_traces_corr ON traces(correlation_id);
CREATE INDEX idx_traces_domain ON traces(source_domain);
```

### Trace Query

`GET /api/chorus/trace/:correlationId` returns all hops ordered by hop number. One query, full picture.

### Auto-Populated Integrations

Domain page integrations derived from trace data:
```sql
SELECT DISTINCT source_service, dest_service, call_stack, COUNT(*) as frequency
FROM traces WHERE source_domain = :domain
GROUP BY source_service, dest_service, call_stack
```

No manual declaration. The integration map is a view over observed traffic.

## Consequences

- Every message is traceable end-to-end via correlation ID
- Hop-level latency shows exactly where slowness or failure occurs
- Domain integrations are observed, not declared — always current
- Common error handling prevents the "warn → retry 17 times → silent" pattern
- Four call stacks share one format — UI, integration, batch, and convergence all trace the same way
- The envelope is infrastructure, not policy — baked into the plumbing, no opt-in required

## Rollout

Spike: instrument seeds pipeline (5 hops). Prove the pattern. Then roll out mechanically to nudges, board events, bridge, crawler, UI page renders, and convergence flows.
