# Services — Service Design

**Wren, 2026-04-22. Draft. Source: chorus.ttl (authorization graph merged today), roles-service-design.md (2026-04-17), pulse-service-design.md (2026-04-20), borg-service-design.md (2026-04-15), #2452 MCP spike, athena-subdomain-completeness.ts, observed running services.**

## Promise

Every Chorus service declares itself once in the graph: `chorus:Service` instance with label, implementedIn, exposesVia, healthAt, ownedBy, dependsOn, consumes, consumedBy. Two surfaces generate automatically — HTTP (for humans/browsers, existing shape) and MCP (for agents, agent-native discovery). A role or an external MCP client enumerates the registry and knows what exists, with typed schemas, without URL-guessing or source-grepping.

When Services is healthy, adding a new service = declaring in graph → appearing in both surfaces on next `/api/athena/reload`. When it drifts, agents and humans rediscover the same system differently, consumers hard-code URLs that eventually rot, and Jeff becomes the registry (explaining "oh, that's actually at /api/loom/... now") every time.

## Overview

Services is the capability-exposure layer of Chorus. Every running service, hook, daemon, or API is an instance; every consumer reads through the registry rather than hard-coded knowledge. It depends on Fuseki for the graph, chorus-api for the generator, and #2442 pulse-style event-bus-cache for invalidation.

| Component | Status | Source | Gap |
|-----------|--------|--------|-----|
| `chorus:Service` class + object properties | MISSING | — | Class not declared in chorus.ttl; exposesVia / healthAt / dependsOn properties don't exist yet |
| Service instances populated | MISSING | Services exist in reality; not in graph | ~15 running services (chorus-api, chorus-hooks, pulse, nudge, observer, clearing, messaging, mcp-registry, Fuseki, deep-health, watchdog, session-indexer, etc.) with ad-hoc declarations scattered across plists, scripts, bats tests |
| HTTP registry surface | PARTIAL | `/api/chorus/*`, `/api/loom/*`, `/api/athena/*` exist | No introspection endpoint; no `exposesVia` metadata; browsers know URLs by convention |
| MCP registry surface | SPIKE | `platform/spikes/mcp-registry/` (#2452) | Proof-of-concept works end-to-end; no generator from graph yet; stdio transport only |
| Generator (graph → surfaces) | MISSING | — | Blocking both surfaces being live-from-graph; hand-declared today |
| Cache + invalidation | MISSING | — | Generator will need event-bus cache per Silas's #2452 feedback; reload-hook on `/api/athena/reload` is the trigger |
| Auth / role-scoping | PARTIAL | CLAUDE.md prose scopes `/gate-product` to Wren, etc.; no code enforcement | Per-role filtering on MCP `listTools` / `callTool` needs trusted side-channel identity |
| Health signals per service | PARTIAL | deep-health.sh per machine; pulse snapshots | Not queryable per-service; no alert on service disappearing from registry |
| Services domain page | MISSING | Gap 7 in roles-service-design (2026-04-17): "chorus domain services section empty" | Services section on chorus domain page renders nothing; no honest-fold label |

## Sub-Domain Interaction Model

Services provides the registry surface; every other sub-product reads (or will read) through it.

| Trigger | Produces | Consumed By | Surface |
|---------|----------|-------------|---------|
| Service declared in graph | `chorus:Service` instance + edges in `urn:chorus:ontology` | Generator (next reload), Domain page (integrations section), MCP clients | chorus.ttl → Fuseki |
| `/api/athena/reload` fires | Generator rebuilds registry cache | MCP registry, HTTP route table | cache invalidation event |
| Consumer calls `listTools` / `listResources` (MCP) | Returns registry entries filtered by caller identity | Agents, external MCP clients | `chorus://*` URIs + tool schemas |
| Consumer calls `/api/chorus/*` (HTTP) | Returns data per existing envelope | Browsers, human tooling | HTTPS |
| Service health probe (`healthAt`) fails | Alert + status update in graph | Pulse, deep-health, Borg observability | spine event |
| Service renamed / retired | Graph instance updated or removed + retire-edge added | Both surfaces regenerate; honest-fold renders "retired" explicitly | reload event |
| New skill / gate declared (via #2348 pattern) | `chorus:Skill` or `chorus:Gate` instance with `implementedIn` | Generator emits MCP tool declaration for it | same graph, same generator |

Core pattern: Services doesn't enforce behavior; it **surfaces truth** about what capabilities exist. Two surfaces, one source. Consistent with Pulse's "assembler not generator" shape (actually emits registry as cache, same family as #2442 sidecar).

## Dependencies

Per Silas's gate:arch amendment (3): `chorus:dependsOn` edges split by target type so lifecycle-coupling is unambiguous. Service-level deps are lifecycle-coupled (target down → source degraded); subdomain-level deps are read-only/substrate references.

### Service-level deps (lifecycle-coupled)

| Dependency | Type | Why |
|-----------|------|-----|
| Fuseki | `chorus:Service` | Registry source — services declare + read here; Fuseki down = registry unavailable |
| chorus-api | `chorus:Service` | Hosts both HTTP endpoints and (future) MCP server; chorus-api down = both surfaces down |
| Pulse | `chorus:Service` | Cache-invalidation signal; Pulse down = stale registry state |
| Reload endpoint | `chorus:Service` (subset of chorus-api) | `/api/athena/reload` triggers generator refresh |

### Subdomain-level deps (substrate references, read-only)

| Dependency | Subdomain | Why |
|-----------|-----------|-----|
| loom-ontology / chorus:ontology | subdomain | Graph content (class declarations, instances) |
| loom-skills, loom-gates | subdomains | Skills + Gates are service-shaped; same generator enumerates them from these subdomains |

Both edge classes use `chorus:dependsOn` (range is broad — `chorus:Service` OR `chorus:SubDomain`) with row annotation making the coupling type explicit. Alternative considered: two properties (`chorus:dependsOnService`, `chorus:dependsOnSubdomain`). Single broad property with typed rows keeps query patterns simple; revisit if lifecycle queries need discrimination. Closes Gap 9 pattern from roles-service-design, extends #2448 scope.

## Components

### `chorus:Service` class (new)

Class declaration in chorus.ttl: `chorus:Service a owl:Class ; rdfs:label "Service" ; rdfs:comment "A running capability that exposes itself via one or more surfaces..."`.

### `chorus:Surface` class + individuals (new)

Per Silas's gate:arch amendment (1): surface must be a declared class with individuals, not a string enum — otherwise graph queryability is lost. Same pattern as `chorus:UtilitySkill` yesterday.

- `chorus:Surface a owl:Class ; rdfs:label "Surface"` — the kind-of-access-surface concept
- Individuals: `chorus:surface-http`, `chorus:surface-mcp`, `chorus:surface-internal`
- Enables clean queries like `SELECT ?s WHERE { ?s chorus:exposesVia chorus:surface-mcp }`

### Object + datatype properties

**New (2):**
- `chorus:exposesVia` — domain `chorus:Service`, **range `chorus:Surface`** (NOT `xsd:string`)
- `chorus:healthAt` — domain `chorus:Service`, range `xsd:string` (URL/endpoint for health check)

**Existing, reused at service scope:**
- `chorus:dependsOn` — pattern from #2448; reused here for service→service + service→substrate edges
- `chorus:consumes`, `chorus:consumedBy` — service-level read/write relationships
- `chorus:implementedIn` — existing (#2348); file path to the service's code entry point

### Service instances

Target population for v1 declaration (one pass, not per-card):

| Service | implementedIn | exposesVia | healthAt |
|---------|--------------|-----------|---------|
| chorus-api | platform/api/src/server.ts | both (http today, mcp when #2452 generator lands) | GET /api/chorus/health |
| chorus-hooks | platform/services/chorus-hooks/src/main.rs | internal | launchctl status |
| pulse | pulse.rs in chorus-hooks | internal | /tmp/pulse-latest.json freshness |
| nudge | platform/services/messaging | http | GET /api/messaging/health |
| observer | platform/services/chorus-hooks/src/observer.rs | internal | declared.json freshness |
| clearing | directing/clearing/src/server.ts | http | GET /health |
| mcp-registry (future) | platform/spikes/mcp-registry/server.mjs | mcp | MCP ping |
| Fuseki | external | internal | GET /$/ping on :3030 |
| deep-health | platform/scripts/deep-health.sh | internal | launchd status |
| watchdog | platform/services/watchdog | internal | spine event freshness |
| session-indexer | platform/services/chorus-hooks | internal | index.db mtime |
| ... | ... | ... | ... |

Populate incrementally. Each service gets one TTL declaration; no mass migration required.

### Generator

SPARQL over `urn:chorus:ontology` returning every `chorus:Service`, `chorus:Skill`, `chorus:Gate`, `chorus:Endpoint` (if declared). Output: MCP tool/resource declarations + HTTP route metadata. Runs on chorus-api startup + on `/api/athena/reload`. Fail-loud if any Service / Skill / Gate has missing `implementedIn` (honest-fold at the registry level per Silas's #2452 feedback).

### Registry cache

In-memory on chorus-api process. Event-bus shape (same as #2442 pulse sidecar). Invalidated by `/api/athena/reload`. Serves both MCP `listTools` and introspection HTTP endpoints cheaply.

### Two surfaces

- **HTTP (existing + augmented):** `/api/chorus/*`, `/api/loom/*`, `/api/athena/*` keep serving today's shape; one new introspection endpoint `/api/chorus/services` returns the registry for browser consumers.
- **MCP (new, generated):** stdio for spike; HTTP+SSE for production (per Silas). Caller identity via trusted side-channel (UDS peer-cred or harness-injected env). Per-role filtering on `listTools`.

## Surfaces

| Surface | Consumers | Transport | Status |
|---------|-----------|-----------|--------|
| HTTP (existing routes) | Browsers, human tooling, curl | HTTPS on :3340 | REAL |
| HTTP introspection | Same | `/api/chorus/services` (new) | MISSING |
| MCP registry | Agents (wren/silas/kade), external MCP clients, portable-Chorus (#1842) | stdio (spike), HTTP+SSE (target) | SPIKE (#2452) |

## Consumers

| Consumer | Reads | Status |
|----------|-------|--------|
| Agent session boot | Would query registry on start; today reads CLAUDE.md inlined | NOT WIRED |
| Domain page (integrations section) | Would populate from `chorus:Service` + `exposesVia` edges | NOT WIRED — Gap 7 |
| Pulse | Service state summary | PARTIAL (deep-health snapshot only) |
| Borg observability | Per-service health + coverage | PARTIAL (per-machine, not per-service) |
| External MCP clients | Enumerate + call | NOT WIRED (spike only) |
| Doc-catalog | Link to each service's source | PARTIAL (manual registration) |
| `/pull` skill | Domain context query | WIRED (partial — queries subdomain, not service directly) |

## Gaps

1. **No `chorus:Service` class in ontology.** Every service today is ad-hoc prose in a plist, script, or CLAUDE.md reference.
2. **Services section of every domain page is empty.** Gap 7 from roles-service-design.md (2026-04-17) — chorus domain renders no services despite ~15 running services.
3. **No `exposesVia` edge.** HTTP-vs-MCP surface attribution isn't queryable; consumers can't ask "what's available via MCP?" without reading prose.
4. **No generator.** Both surfaces (HTTP today, MCP target) are hand-maintained. Adding a service means editing server.ts, updating plists, maintaining CLAUDE.md — four different surfaces for one concept. Classic competing-implementations pattern (#2437).
5. **No cache invalidation on service state changes.** Registry (if it existed) would drift silently.
6. **Auth mapping undefined.** Per-role scoping works by convention for skills (Wren runs gate-product); no code enforcement for MCP `listTools` filtering by caller identity.
7. **Health signal per service missing.** deep-health tracks machine-level; no per-service alert on "service disappeared from registry" or "service declared but unreachable."
8. **No formal retire-edge.** When a service is retired (like #2283 nudge consolidation removed paths), the graph should carry `chorus:retiredAt` + `chorus:supersededBy`. Today retirement is prose + code-removal.
9. **No domain page render for services section.** Honest-fold discipline not applied — absence renders nothing rather than "0 services declared (query-empty)".
10. **`chorus:dependsOn` edges at service level.** The substrate pattern (#2448) extends down — service-A-dependsOn-service-B for lifecycle coupling. Not declared today.

## Next Steps

| # | Action | Impact | Owner | Depends |
|---|--------|--------|-------|---------|
| 1 | Declare `chorus:Service` class + `exposesVia`/`healthAt` properties in chorus.ttl | Schema ready for instance population | Wren + Silas review | — |
| 2 | Populate ~15 service instances (one-pass, minimal envelope) | Services domain has real data; Gap 7 closes | Wren | #1 |
| 3 | Generator (SPARQL → MCP tool/resource declarations + HTTP route metadata) with fail-loud on missing implementedIn | Both surfaces live from graph; no hand-maintenance | Wren (conceptual) + whoever-builds | #2; unlocks #2452 follow-on |
| 4 | Event-bus cache + `/api/athena/reload` invalidation hook | Registry stays fresh at scale; same shape as #2442 pulse | Silas (owns event-bus pattern) | #3 |
| 5 | Auth: trusted side-channel caller identity; per-role filtering on MCP listTools/callTool | Secure adoption of agent surface | Silas | #4 |
| 6 | Transport: HTTP+SSE on chorus-api replacing stdio | Production-ready multi-client | Silas | #5 |
| 7 | Invocation safety: MCP tool calls delegate to chorus-hook-shim with existing gate chain | Real tool invocation possible (not just dry-run) | Silas+Wren pair | #6 |
| 8 | Domain page services section renders honest-fold from registry | Gap 7 closes visibly | Wren (conceptual) | #3 |
| 9 | Retire-edge (`chorus:retiredAt`, `chorus:supersededBy`) + retirement-gate | Competing-implementations #2437 closes structurally for services | Silas | — (parallel) |

Implementation cards per Next Step, not on a single mega-card. Each Step is one pull's worth of work.

**Parallelization (per Silas gate:arch nit):** Steps 4-7 are currently assigned sequentially to Silas (cache → auth → transport → invocation-safety) — long chain on one role. Parallel work possible: Wren's step 3 (generator) can run in parallel with Silas's step 4 (cache); step 8 (domain page render, Wren/Kade) can start as soon as step 3 ships. Steps 5-7 (auth, transport, invocation) remain genuinely sequential — each depends on the previous.

**Client-layer consolidation step (added per amendment 2):** Before step 1 implementation card is filed, an explicit Next Step or a pointer to an authoring design must cover "shared invocation SDK per capability" — otherwise the declaration layer ships while internal consumers keep reinventing Vikunja clients. Recommend: file a separate design card for client-layer consolidation; this design stays focused on declaration.

## Not in Scope

- Migrating all HTTP endpoints to MCP-only (collapse-to-MCP question is open; spike was explicit on this). Two surfaces coexist until a separate design decision says otherwise.
- Moving services to a different repo (body-burial work is #2041 / #2291 / #2445 family; this design assumes current repo topology).
- New services (this design names the shape for services that exist; adding new ones follows the shape but is per-service cards).
- Cross-machine registry federation (Bedroom services are currently observable via deep-health; federating their service-registry is a follow-on).
- Performance tuning (32 skills today; re-evaluate when service count crosses ~100).
- **Client-layer consolidation (per Silas gate:arch amendment 2).** This design covers the *declaration* layer — what services exist, how they declare themselves, how both surfaces generate from one source. It does NOT cover the *client-layer consolidation* surfaced this morning (2026-04-23): cards-CLI and chorus-api both speak to Vikunja with independent client code; multiple internal consumers reinvent the same capability client. "Two surfaces, one source" only holds if the consumers delegate to one shared client per capability — which the current codebase does not. This needs its own design + DEC before any step-1 implementation card is filed, OR a Next Step gets added to this design (see below). **Referenced explicitly so it doesn't go silent.**
