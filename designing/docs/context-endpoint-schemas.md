# Context Endpoint Schemas — Minimum Set

**Silas, 2026-04-19. Under #2234 Step 2. Follows `context-api-endpoint-audit.md` (Step 1). Defines the minimum Context endpoint set with declared common-envelope responses.**

## Scope

Three endpoints for the proof-of-shape. They cover the highest-hallucination-risk questions roles ask: "what's WIP", "what's everyone doing", "is anything broken". If these three work reliably + reused + valuable, the pattern generalizes.

**Minimum set:**

1. `GET /api/chorus/context/board/wip` — current WIP state
2. `GET /api/chorus/context/roles` — current role states
3. `GET /api/chorus/context/health` — system health signal

Everything else (cost, quality, perf, freshness, analytics, coverage, logs, alerts, etc.) follows the same shape once the pattern is proven — not part of the proof-of-shape itself.

## Common Envelope (applies to every response)

```json
{
  "step": "building",
  "product": "chorus",
  "domain": "chorus",
  "subdomain": "chorus-hooks",
  "timestamp": "2026-04-19T10:15:00-04:00",
  "source": "/api/chorus/context/board/wip?role=silas",
  "data": { ... endpoint-specific ... }
}
```

**Four canonical-model levels, all optional by scope:**

| Field | Source | When present | Reference-model equivalent |
|-------|--------|--------------|---------------------------|
| `step` | Athena named graph (Fuseki `/pods`) | whenever a domain is known | value-stream step |
| `product` | Athena named graph | whenever a domain is known | product (Gathering/Chorus/Borg) |
| `domain` | Athena named graph (the sub-product node) | when scoped to a domain | **sub-product** (chorus, photos, music) |
| `subdomain` | Athena named graph (the sub-domain node) | when scoped to a subdomain | sub-domain / module (chorus-hooks, photos-metadata) |

**Naming note:** `domain` in this schema corresponds to **sub-product** in the Chorus reference-model diagram (chorus-context-diagram-v2). The term "domain" is used here because it aligns with API conventions and the DOMAIN_REGISTRY vocabulary already in the codebase. Readers of the reference model should map: reference-model sub-product ↔ envelope `domain`.

System-scoped responses (`/context/health`, `/context/roles`) carry step + product only — no domain or subdomain. Domain-scoped responses add domain. Subdomain-scoped add all four.

**`source`** — the URL that produced this response. Enables agent citation: "per /api/chorus/context/board/wip?role=silas @ 10:15, silas has 2 WIP."

**`timestamp`** — ISO8601 with timezone. Always present.

**`data`** — the payload. Shape specific to the endpoint, flat and named.

**Fields are omitted, not null, when they don't apply.** A consumer should not expect all four — scan for what's present.

## Endpoint 1: `GET /api/chorus/context/board/wip`

**Question answered:** What cards are in WIP right now? Optionally scoped to one role.

**Query params:**
- `role` (optional) — filter to one role (silas | wren | kade)

**Response shape:**

```json
{
  "valueStream": "coordination",
  "step": "building",
  "product": "chorus",
  "timestamp": "2026-04-19T10:15:00-04:00",
  "source": "/api/chorus/context/board/wip?role=silas",
  "data": {
    "total": 2,
    "cards": [
      {
        "id": 2218,
        "owner": "Silas",
        "title": "Codesign chorus-hook-shim + chorus-inject with stable identifiers",
        "priority": "P1",
        "domain": "chorus",
        "valueStream": "ops",
        "step": "observability",
        "createdAt": "2026-04-19T10:46:30Z",
        "updatedAt": "2026-04-19T12:59:43Z"
      },
      {
        "id": 2234,
        "owner": "Silas",
        "title": "Move chorus API from attic to workbench",
        "priority": "P1",
        "domain": "chorus",
        "valueStream": "coordination",
        "step": "framework",
        "createdAt": "2026-04-19T13:20:00Z",
        "updatedAt": "2026-04-19T14:05:00Z"
      }
    ]
  }
}
```

**Source today:** Vikunja board API (via `cards list --status WIP`). Also mirrored in `/tmp/pulse-latest.json:board.wip_cards`.

**Shape rule:** Flat cards, no nesting. Each card carries its own canonical-model fields (domain/valueStream/step) stamped from the graph. Fields are named, not abbreviated. Ordered by `id` ascending.

**Staleness contract:** Fresh-on-read from the board API (single source of truth). No cache. If the board API is slow, the response is slow — preferable to returning stale-but-fast.

## Endpoint 2: `GET /api/chorus/context/roles`

**Question answered:** What is each role doing right now?

**Query params:** none (single call returns all three roles).

**Response shape:**

```json
{
  "product": "chorus",
  "domain": "chorus-roles",
  "timestamp": "2026-04-19T10:15:00-04:00",
  "source": "/api/chorus/context/roles",
  "data": {
    "roles": [
      {
        "name": "silas",
        "state": "building",
        "card": 2234,
        "gemba": null,
        "lastActivity": "2026-04-19T10:14:32-04:00",
        "lastEvent": "card.demo.started"
      },
      {
        "name": "wren",
        "state": "building",
        "card": 2230,
        "gemba": null,
        "lastActivity": "2026-04-19T10:14:58-04:00",
        "lastEvent": "card.pulled"
      },
      {
        "name": "kade",
        "state": "observing",
        "card": null,
        "gemba": "silas",
        "lastActivity": "2026-04-19T10:15:02-04:00",
        "lastEvent": "gemba.started"
      }
    ]
  }
}
```

**Source today:** `/tmp/claude-team-scan/{role}-declared.json` (push from role-state script) merged with spine-event tail for `lastEvent` / `lastActivity`.

**Shape rule:** Array of roles, not a map keyed by role name. Arrays are easier to iterate and sort; maps force callers to know the key set ahead of time. `card` is `null` when idle; `gemba` is `null` when not observing — explicit, not omitted, because "not set" is a meaningful state.

**Staleness contract:** `lastActivity` explicit so consumer sees freshness. If `lastActivity` is >5min old, role state is suspect — consumer decides how to handle.

## Endpoint 3: `GET /api/chorus/context/health`

**Question answered:** Is anything broken or degraded right now?

**Query params:** none.

**Response shape:**

```json
{
  "product": "chorus",
  "domain": "chorus-health",
  "timestamp": "2026-04-19T10:15:00-04:00",
  "source": "/api/chorus/context/health",
  "data": {
    "status": "degraded",
    "failures": 1,
    "warnings": 2,
    "summary": "chorus-api hung 14h (fixed); loki-bedroom probe slow",
    "checks": [
      {
        "name": "chorus-api",
        "status": "ok",
        "latencyMs": 12,
        "lastCheck": "2026-04-19T10:14:50-04:00"
      },
      {
        "name": "loki-bedroom",
        "status": "warning",
        "detail": "non-localhost response time 4.4s exceeds probe threshold 3s (loki slow, not unreachable)",
        "lastCheck": "2026-04-19T10:14:55-04:00"
      },
      {
        "name": "fuseki",
        "status": "ok",
        "latencyMs": 34,
        "lastCheck": "2026-04-19T10:14:50-04:00"
      }
    ]
  }
}
```

**Source today:** `deep-health.sh` output + `/tmp/deep-health-last-failures.txt` + in-process probe-data once `ops-as-domain` work lands.

**Shape rule:** Top-level `status` is one of `ok | degraded | down` (three values, not a flood of nuance). `failures` and `warnings` are counts — consumer knows scope at a glance without reading checks. `checks` array gives detail; each check has a specific `status` (ok | warning | error | unknown), human-readable `detail` for non-ok, latency where measured. Alert *framing* (Jeff's morning feedback) stays out of this response — the consumer does the framing.

**Staleness contract:** `lastCheck` per entry. If ALL `lastCheck` are older than the probe cadence (~60s), the probe itself is unhealthy — consumer surfaces that as meta-health.

## Schema Artifacts

Each endpoint ships with an OpenAPI fragment (or equivalent JSON schema) in the handler file, not in a separate doc directory. When the schema drifts from the handler, lint catches it. Example for Endpoint 1:

```yaml
paths:
  /api/chorus/context/board/wip:
    get:
      summary: Current WIP cards, optionally scoped to a role
      parameters:
        - name: role
          in: query
          required: false
          schema:
            enum: [silas, wren, kade]
      responses:
        '200':
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BoardWipResponse'
components:
  schemas:
    ContextEnvelope:
      type: object
      required: [timestamp, source, data]
      properties:
        valueStream: { type: string }
        step: { type: string }
        product: { type: string }
        domain: { type: string }
        timestamp: { type: string, format: date-time }
        source: { type: string }
        data: { type: object }
    BoardWipResponse:
      allOf:
        - $ref: '#/components/schemas/ContextEnvelope'
        - type: object
          properties:
            data:
              type: object
              required: [total, cards]
              properties:
                total: { type: integer }
                cards:
                  type: array
                  items: { $ref: '#/components/schemas/WipCard' }
    WipCard:
      type: object
      required: [id, owner, title, priority, domain]
      properties:
        id: { type: integer }
        owner: { type: string }
        title: { type: string }
        priority: { enum: [P1, P2, P3] }
        domain: { type: string }
        valueStream: { type: string }
        step: { type: string }
        createdAt: { type: string, format: date-time }
        updatedAt: { type: string, format: date-time }
```

Schema shipped alongside handler is the interface contract — the consumer can generate a typed client, the lint can catch drift, the docs exist automatically.

## Headers That Stamp Canonical Metadata

Every response carries a shared envelope stamped with canonical-model fields. The stamping helper is `stampHeader` — async, one function, every Context handler calls it.

### Source: Athena named graph in Fuseki (single source of truth)

Canonical metadata lives in OWL/RDF — the Athena named graph in Fuseki `/pods`. The same data source Athena handlers already query. `DOMAIN_REGISTRY` in server.ts is NOT the source — it duplicates graph data and splits ownership. `stampHeader` queries the graph directly:

```typescript
// platform/api/src/lib/context-envelope.ts
import type { AthenaSparqlClient } from '../athena-sparql';

export async function stampHeader(
  client: AthenaSparqlClient,
  domainId: string | null,
  subdomainId?: string | null,
): Promise<ContextEnvelopeFields> {
  if (!domainId) {
    return { timestamp: new Date().toISOString() };
  }
  const result = await client.query(`
    PREFIX chorus: <urn:gathering:chorus#>
    SELECT ?product ?step WHERE {
      GRAPH <${ATHENA_GRAPH}> {
        ?d chorus:name "${domainId}" ;
           chorus:product ?product ;
           chorus:step    ?step .
      }
    } LIMIT 1
  `);
  const b = result?.results?.bindings?.[0];
  return {
    ...(b?.step?.value    && { step: b.step.value }),
    ...(b?.product?.value && { product: b.product.value }),
    domain: domainId,
    ...(subdomainId       && { subdomain: subdomainId }),
    timestamp: new Date().toISOString(),
  };
}
```

If a domain has no graph data yet, the envelope omits step/product gracefully — no crash, no fallback to the TS object.

**Hermetic test pattern:** Inject an `AthenaSparqlClient` stub whose `query()` returns fixture bindings. Same dep-injection pattern every Athena handler uses. No live Fuseki needed in tests.

For endpoints not scoped to a domain (e.g., `/context/roles`), `stampHeader(client, null)` returns `{ timestamp }` only.

## Implementation Notes for Kade (Services vertical, Step 3)

- Create `platform/api/src/handlers/context-*.ts` — one per endpoint.
- Create `platform/api/src/lib/context-envelope.ts` — shared envelope builder + SPARQL-stamp helper.
- OpenAPI fragments inline in handler files (annotation or adjacent `.yaml` file).
- **Correction (post-Step-1-audit):** `/api/chorus/pulse` and `/api/chorus/role-state` exist as POST only, not GET. No aliasing — create the new GETs fresh, reading the same underlying sources (Vikunja board API for /context/board/wip, `/tmp/pulse-latest.json` + role-state files for /context/roles). Only `/api/chorus/pulse/latest` is an existing read path; that stays untouched for now.
- Tests under `platform/api/tests/handlers/context-*.test.ts` — hermetic per TEST.md, fixture TTL for SPARQL stamping.

## Next Step (per #2234 Implementation Outline)

Step 3 is Kade's (Services vertical). Brief Kade with this doc + audit when Jeff gives the go signal to move into implementation. Remaining steps (4–7) depend on Step 3 landing.

## References

- `context-service-design.md` — parent design
- `context-api-endpoint-audit.md` — Step 1 output (endpoint inventory)
- `chorus-overview.md` — refreshed service design
- #2234 — the card
