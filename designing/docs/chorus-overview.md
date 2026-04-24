# Chorus — Team Coordination Product

**What it is:** An authored body of work — the operating system for a human+AI team, expressed as versioned, legible, auditable protocol. The protocol IS the product. The gathering-team repo is the Chorus Werk — not a repo that contains it.

**Named:** DEC-019 (2026-02-17). The name reflects coordinated voices, not synchronized — and pride in craftsmanship.

**Positioning (DEC-034):** Chorus is a Werk, not a tool. Three defining qualities: **versioned** (like code), **legible** (like prose), **auditable** (like infrastructure). Every artifact must pass all three. "Werk" from Heidegger — the work that discloses truth through its making.

**Patent lineage:** US9552400B2 (Bridwell, Staples 2012-2017). Same architectural pattern (RDF/OWL + SPARQL + approval gates) validated at enterprise scale. Staples cleanup lessons are load-bearing for Chorus design — the common envelope, canonical-metadata-in-header, domain/value-stream as data not URL path, interface-design-as-practice are all inherited discipline.

---

## Product Vision

Chorus is the nervous system connecting direction to delivery and back. The value stream is the spine. Every capability — hooks, scripts, dashboards, briefs, templates, gates, APIs — is either a nerve that carries a signal along that spine or a contract at a junction.

Every touchpoint either:
- **Senses** something (commit, deploy, error, Slack message, nudge)
- **Routes** it (to the right role, card, dashboard, endpoint)
- **Constrains** it (gates block bad actions at the edge)
- **Proves** it (screenshot, dashboard, audit trail, retrospective, CI pipeline run)

Session templates (demo, board walk, review, SWAT) aren't meeting formats — they're **interaction protocols that shape how signal flows through the spine.** Quality gates, pre-commit hooks, CI/CD, monitoring, alerting aren't a separate "ops" concern — they're the Proving stage closing the loop back to Directing.

**Design principle:** Lightweight constraints built into the flow, not process docs that rely on willpower. The rules live in the wiring. If a role can skip it, it's aspirational. If a hook or gate enforces it, it's real.

---

## Architectural Shape

Chorus decomposes along **two orthogonal dimensions**: horizontal layers (dependency stack) and vertical capabilities (cross-cutting). See `chorus-context-diagram-v2.html` for the canonical visual.

### Horizontal layers (top depends on bottom)

| Layer | What | Owner |
|-------|------|-------|
| **Shared Awareness** | History, sessions, stories, decisions, search, memory — indexed and queryable. The product consumers experience. | All / Chorus |
| **OWL/RDF (dashed)** | Semantic layer. Ontology gives signals meaning: domain→service→gate→role. Spans all layers — not a service you call, the schema giving structure to everything above and below. | All (OWL) |
| **Loom** | Roles, Clearing, interactions, briefs, cards, skills, policies, decisions, practices. How three AI roles and one human coordinate without Jeff becoming the relay. | Silas → Kade (app code) |
| **Protocol** | Spine events + gates. Every action emits a signal; every transition is gated. | Silas |
| **Framework** | ICDs, Service Design, Domain Pages, BDD Gates. Contracts defined before code. | All (OWL) |
| **Apps** | Gathering, Chorus, Borg. Consumers of services; emit spine events. | Kade |
| **Observability** | Alerts, logs, health, metrics. Roles see problems before Jeff does. | Silas |
| **Infrastructure** | Two Mac minis (Library + Bedroom), LaunchAgents, Cloudflare tunnel, deploy tooling. Everything above depends on this staying up. | Silas |

**Silas logjam:** 4 of 8 horizontal layers. Risk to velocity when cross-layer work stacks.

### Vertical capabilities (span all layers)

| Vertical | What | Owner |
|----------|------|-------|
| **Services** | API, Endpoints, UIs, Domains. The consumer surface of every layer. | Kade |
| **Quality** | Tests, Gates, Coverage, SLAs. The correctness surface of every layer. | Kade |

Verticals cut through all horizontal layers. Every layer has services and every layer has quality concerns — not one team's responsibility to exist, but one team's responsibility to shape and maintain coherence.

### Three sub-domains (cross-cutting surfaces)

Shared Awareness decomposes into three distinct consumer surfaces, each with its own failure mode and service design:

| Sub-domain | What | Fails → |
|------------|------|---------|
| **Memory** | Persistent state across sessions. ~/.claude/.../memory/, next-session.md, briefs, activity.md, chorus SQLite index, state files. | Sessions start cold, continuity lost. |
| **Context** | What's in the agent's window this turn. Envelope injection, pulse snapshot, recent spine, athena domain synthesis. | Agent hallucinates or ignores delivered data. |
| **Knowledge** | What's true about the system. Fuseki graph, canonical model, domains, ICDs, decisions. | Graph drifts from reality, no source of truth. |

They interoperate: Knowledge → Context (pulse reads graph-grounded state); Memory → Context (cross-session continuity); Context → Memory (briefs, decisions written back); Context → Knowledge (spine events accrete into graph).

Each sub-domain earns its own service design doc. Context design is #2234.

**Alerts sub-domain: intended, not yet populated.** Intended to live under Observability with its own service design.

---

## Interface Design

Interface design is the discipline of shaping the **consumer surface** of every horizontal and vertical, from the consumer-in, not from the implementation-out. Historically this has been done ad hoc in Chorus — endpoints grew organically, shapes were accidental, and APIs drifted into attic (built, not load-bearing). This changes going forward.

### URL taxonomy — question-shaped, not storage-shaped

Top-level Chorus API reflects the three sub-domains:

```
/api/chorus/memory/...      — persistent state surfaces
/api/chorus/context/...     — per-turn agent-facing synthesis
/api/chorus/knowledge/...   — graph / canonical model
```

Level 2 by agent-question: `/context/board/wip`, `/context/roles`, `/context/health`, `/knowledge/domains`, `/memory/briefs`. Level 3 scopes: `/context/board/wip?role=silas`, `/knowledge/domains/photos`.

Rule: the URL is a promise about the question it answers. Storage shape is not consumer shape.

### Common envelope — canonical metadata in the header

Every API response carries a uniform header stamped from the canonical model:

```json
{
  "valueStream": "curation",
  "step": "harvesting",
  "product": "photos",
  "domain": "photos-metadata",
  "timestamp": "2026-04-19T09:20:00-04:00",
  "data": { ... }
}
```

Fields populate where they make sense — product-level responses have no `domain`; responses with no canonical-model home have just `timestamp`. Graph is the source; endpoints look up the parent chain and stamp. Same header across API responses, alert payloads, nudge metadata, spine event records. Consumers read the top band at glance-speed and drop into data when needed.

Inherited from Staples ESB pattern — one header, canonical-model-driven, consumer-uniform.

### Three axes that must land together

For any Chorus interface to pass the trinity (below):

1. **Structure** — endpoints shaped around agent questions, not storage layout. URL is the spec.
2. **Data** — source correctness. Faster access to wrong data is worse than slower access to right data.
3. **Presentation** — flat, named, small. Optimized for the consumer's parsing budget.

### Pull over push

Old model: push large pre-synthesized context into every turn; hope the agent reads it. New model: inject a small manifest; agent queries the API at point-of-need; claims grounded in fresh data.

Push model wastes context budget and trains agents to ignore content because there's too much. Pull model puts consumption at the moment the question is asked — mechanically prevents hallucination because the agent has to fetch to claim.

### Interface design as sustained practice

- Every new endpoint passes a named review against the taxonomy before landing.
- Every response shape gets an OpenAPI / schema artifact shipped with the handler.
- `gate:interface` (named, separate from `gate:arch` and `gate:code`) is the future addition to the gate registry.

---

## Architectural Axes

Two axes for diagnosing any surface:

### Push vs Pull
- **Push** — delivered to consumer without being asked. Envelope injection, SessionStart hooks.
- **Pull** — consumer asks at point-of-need. API call.

Most of Chorus today is push. Pull is where the weight is moving (#2234).

### Attic vs Workbench
- **Attic** — built, stored, rarely the active surface. Agents reinvent instead of reaching for it.
- **Workbench** — load-bearing, consulted routinely, the first place a question goes.

A surface can be perfectly well-built and still be attic. Jeff's metaphor. The test: when a role has a question this capability answers, is the capability the first place they look, or do they invent?

---

## Evaluation Lens

### The trinity: reliable / reused / valuable

Every horizontal layer and every vertical capability must pass all three. Any one that fails makes the whole system only as good as its weakest link.

| Property | What it means | Fails → |
|----------|---------------|---------|
| **Reliable** | Trustworthy, stable, doesn't break. Consumers can depend on it. | Load-bearing on sand. |
| **Reused** | Consumers lean on it; not copy-pasted, not re-invented per use. | Scaling effort no one consumes. |
| **Valuable** | Actually does useful work; earns its tax. | Ceremony for its own sake. |

The three are interdependent:
- Reliable without reused = heroic one-off, ages badly
- Reused without valuable = scaling something nobody needs
- Valuable without reliable = fragile load-bearing

The test for any surface (new or existing): does it pass all three? Not "is it built?" Built is the floor. Reliable + reused + valuable is the ship condition.

### Weak-link principle

System coherence is bottlenecked by its worst surface. Seven healthy layers and one attic layer means the whole stack is ceiling'd at attic. Interface design must be sustained across every horizontal AND every vertical — every weak link is a ceiling.

---

## Value Stream

```
Directing → Designing → Building → Proving → (loop)
```

Each stage has gates. Work items flow through the pipeline. Trust accrues as items pass gates successfully.

Pipelines per value-stream step (#2219) are the next-level shape — each step gets its own CI/CD lane, timestamped artifacts, proving demo. Athena is the first proving pipeline.

---

## Current State (April 2026)

- **Silas logjam** — 4 horizontal layers own by one role. Cross-layer work serializes.
- **Push model dominant** — context injection fires every turn (~5,435 session.context.built events/week pre-#2231). Consumption uneven.
- **API in the attic** — agents routinely invent board/role/WIP state despite pulse data being in context. Pull model shift via #2234.
- **Canonical model mid-formation** — 24 domains registered, alerts/memory/context sub-domains intended but not yet populated.
- **6 TS projects** — historical accretion, not architecture. Consolidation follows pipeline-per-step work.
- **Value-stream pipelines not yet live** — #2219 is the first. Design before build (DEC, this session).

---

## Core Documents

| Document | Location | Owner | Description |
|----------|----------|-------|-------------|
| Context Diagram v2 | [designing/docs/chorus-context-diagram-v2.html](chorus-context-diagram-v2.html) | Jeff | Canonical visual — horizontals, verticals, seed journey, ownership |
| Team Architecture | [team-architecture.md](../../team-architecture.md) | Silas | Operating model — principles, communication, session lifecycle |
| Roles Service Design | [designing/docs/roles-service-design.md](roles-service-design.md) | Wren | Sub-domain service design: Promise / Overview / components / interaction |
| Quality Service Design | [designing/docs/quality-service-design.md](quality-service-design.md) | Wren | Sub-domain service design |
| Context Service Design (pending) | [designing/docs/context-service-design.md](context-service-design.md) | All | #2234 deliverable |
| Gate Registry | [designing/docs/gate-registry.md](gate-registry.md) | Silas | Enforced gates, checklists, fitness functions |
| Decisions Log | [designing/decisions/](../decisions/) | Wren | DEC-001+ |

## ADRs

| ADR | Location | Description |
|-----|----------|-------------|
| ADR-010 | [architect/adr/ADR-010-generalized-harvest-pipeline.md](../../architect/adr/ADR-010-generalized-harvest-pipeline.md) | Harvest pipeline + quality gates |
| ADR-011 | [architect/adr/ADR-011-production-like-deployment-pattern.md](../../architect/adr/ADR-011-production-like-deployment-pattern.md) | Atomic deploys, health gates, rollback |

## Team Roles

| Role | Person | Focus |
|------|--------|-------|
| Product Manager | Wren | What + why + when |
| Architect | Silas | How + constraints + operations |
| Engineer | Kade | Build + test + ship |
| Director | Jeff | Vision + direction + decisions |

## Board

Run `cards list` or visit http://localhost:3456 (Project: Chorus).

## Logs & Observability

- **Structured logs:** `platform/logs/chorus.log` (JSON, queryable by role/component/level)
- **Loki:** Scraped by Promtail, queryable in Grafana at http://localhost:3100
- **Pulse snapshot:** `/tmp/pulse-latest.json` (board + roles + health rolled up)

---

*Last updated: 2026-04-19 — refresh under #2234 (context service design). Previous version dated 2026-02-20.*
