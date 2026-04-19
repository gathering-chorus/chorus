# Context — Service Design

**Silas + Wren + Kade, 2026-04-19. Draft under #2234. Source: context_inject.rs, pulse-latest.json, chorus-api /api/chorus/* endpoints, session envelope spec, 2026-04-19 session with Jeff.**

## Promise

When Context is healthy, every role reasons on fresh, correctly-attributed data at the moment they need it. Agents don't invent board state, don't hallucinate role activity, don't claim coverage numbers from memory. Claims made in responses are traceable to a specific API call or envelope field with a timestamp. When Context drifts, agents produce plausible-looking output that diverges from reality, and Jeff becomes the corrector — the highest-cost failure mode in the system.

## Overview

Context is the per-turn agent-facing synthesis surface of Chorus. It is distinct from Memory (what persists across sessions) and Knowledge (what's true about the system). Context answers "what's happening right now, in a form the agent can use to form a claim." Everything a role says about current state — board, roles, health, recent activity — is supposed to be grounded in Context.

Context composes data from three directions: Knowledge (graph-backed truth about domains, services, canonical metadata), Memory (cross-session continuity — briefs, prior decisions), and live system state (spine events, role-state files, health probes). It shapes that data into two delivery modes: **push** (envelope injection at SessionStart and UserPromptSubmit) and **pull** (API endpoints queried at point-of-need). The shift from push-dominant to pull-dominant is #2234's thesis.

Context is a cross-cutting capability under Shared Awareness (per chorus-context-diagram-v2). It depends on Knowledge for correctness, Memory for continuity, and the Framework layer for interface contracts (common envelope, URL taxonomy, presentation shape). The Services vertical (Kade-owned) carries implementation; the Framework layer (joint, OWL-backed) carries contract.

**Current state (2026-04-19):** Push-dominant. Envelope injection fires every user prompt; consumption is uneven. Agents routinely form claims that diverge from data actually present in their context window (session 2026-04-19: silas claimed 5 WIP cards while pulse showed 2; the data was in-context and not consulted). Pull endpoints exist but are shaped as implementation, not designed as an API (see "Structure / Data / Presentation" below). The API is "in the attic" — built, not load-bearing.

| Component | Status | Source | Gap |
|-----------|--------|--------|-----|
| Envelope injection (push) | REAL | context_inject.rs — pulse + spine + athena into `<context-synthesis>` on UserPromptSubmit | Consumption uneven; agents skip past; format not optimized for read. Turn-latency tax (#2231 caches; #2234 reduces volume). |
| Pulse snapshot | REAL | /tmp/pulse-latest.json refreshed by daemon; read_pulse_snapshot() formats for inject | Board WIP count + first 5 cards; no filterable slice by role, no endpoint to query fresh outside inject window |
| Recent spine tail | REAL | query_recent_spine() reads platform/logs/chorus.log | File-based; scales poorly; no API surface |
| Athena domain fetch | REAL | query_athena_domain() HTTP to /api/athena/* | Per-turn HTTP (#2231 caches); not agent-question-shaped URLs |
| Hybrid search | REAL | query_chorus_hybrid() HTTP to /api/chorus/search | Same (#2231 caches) |
| Memory file scan | REAL | scan_memory() reads role memory/*.md | Unbounded, not in API; consumption implicit |
| Pull API — /api/chorus/context/* | MISSING | — | Entire sub-tree not yet designed; #2234 deliverable |
| Common envelope header | MISSING | — | Each response today hand-shapes its metadata; no canonical-stamped header |
| URL taxonomy (question-shaped) | PARTIAL | /api/chorus/domain/{id}, /api/chorus/pulse exist; inconsistent elsewhere | Need /context/board, /context/roles, /context/health, /context/spine, /context/nudges |
| Presentation discipline | PARTIAL | Some handlers return flat named shapes; others return deep storage-mirrors | No named response models, no OpenAPI/schema shipped with handlers |
| Observability of consumption | MISSING | — | We don't measure whether agents read injected content; OBS2 per-turn cost measured but hit/miss ratio unknown |
| Alerts sub-domain integration | PARTIAL | Alerts live under Observability; no common-envelope tagging | Alerts should carry same header (domain/valueStream/step) — part of alerts-domain work |

## The Three Axes

Context must pass all three to move from attic to workbench:

1. **Structure.** URL taxonomy matches agent questions, not storage layout. `/api/chorus/context/board/wip?role=silas` reads as "what is silas's WIP" — the URL is the spec. Each URL resolves to a question an agent actually asks.
2. **Data.** Source correctness. Staleness, drift, divergence from Knowledge-graph truth all corrupt the consumer's reasoning. Faster access to wrong data is worse than slower access to right data. Pull model sharpens this — no more "we synthesized it once, hope it's still true."
3. **Presentation.** Response shape optimized for consumer parsing. Flat, named, small. Common envelope at the top (valueStream, step, product, domain, timestamp) stamped from Knowledge. Payload under `data`, shaped for the specific question. No cryptic keys, no mixed types, no deep nesting that exists to mirror storage normalization.

## Push ↔ Pull Mode Contract

| Mode | When | Cost | Appropriate for |
|------|------|------|-----------------|
| **Push (envelope inject)** | Per user prompt, SessionStart | Per-turn latency (#2231 caching mitigates); context-window budget; consumption uncertainty | Small, always-useful orientation signal: timestamp, role, WIP count, current domain, health-one-liner |
| **Pull (API call)** | Point-of-need during response composition | Per-call latency (bounded); mechanically enforces consumption | Any claim that will appear in the response: board state, role state, recent events, domain detail, health details, search results |

**Rule:** if an agent is about to form a claim about current state, the claim must be grounded in a pull, not in pushed content. Pushed content is orientation — "here's where you are" — not citation-ready evidence. Pull endpoints are citation-ready by design (URL + timestamp in response envelope).

Envelope injection shrinks accordingly. Target: envelope under 2KB after reshape, vs. ~16KB today. The envelope becomes a manifest ("here are the endpoints you may want, here's today's orientation signal") rather than a pre-synthesized blob.

## Sub-Domain Interaction Model

### Context ↔ Knowledge

Context leans on Knowledge for:
- **Canonical metadata stamping.** Common envelope header fields (valueStream, step, product, domain) resolve from the graph via SPARQL at endpoint time. No per-endpoint hand-maintained mapping. When Knowledge moves a domain from one step to another, every Context response picks up the change automatically.
- **Domain detail.** `/context/board/wip` includes a card; the card's domain tag resolves to full domain metadata from Knowledge when joined.
- **Correctness.** Context does not hold authoritative state for what a domain IS; it reads from Knowledge.

Knowledge depends on Context for: nothing directly. Knowledge is pull-only — it serves; Context consumes.

### Context ↔ Memory

Context writes to Memory:
- **Briefs authored this session** land in Memory's briefs surface.
- **Decisions made this session** land in Memory's decisions surface.
- **Spine events this session** accrete to Memory's activity log.

Memory writes to Context:
- **Session-start synthesis.** Previous-session next-steps, stale-since dates on pending work, cross-session continuity — Memory provides the baseline that session-start injection uses.

The Context/Memory boundary is the most contested in this design. Role state (current card, observation target, idle state) changes per-turn but persists across sessions. Today it's split: `/tmp/claude-team-scan/*-declared.json` is session-scoped (Context); `next-session.md` is cross-session (Memory). The design rule: **Context owns "what is the state right now"; Memory owns "what persists when nothing is loaded." A piece of data may appear in both, but has one owner per concern.**

### Context ↔ Apps

Apps (Gathering, Chorus, Borg, Kade's domain) are consumers of Context APIs. A dashboard showing "Silas's WIP" calls `/api/chorus/context/board/wip?role=silas` — same URL an agent would call. Uniform consumer surface, regardless of whether the consumer is human-facing UI or agent-reasoning path.

### Context ↔ Observability

Alerts are observability signals. They should carry the common envelope header (valueStream, step, product, domain) stamped from Knowledge. An alert about Loki on Bedroom tags `domain: loki, valueStream: ops, step: observability` — the role receiving the alert correlates at glance-speed against their current WIP or ignores-and-moves-on. Alerts currently do not do this; alerts-domain rework brings them into the common envelope.

## Design Rules

1. **Every Context endpoint carries the common envelope.** Header first, payload under `data`. No exceptions.
2. **Canonical metadata is stamped, not hand-maintained.** Endpoints resolve valueStream/step/product/domain from the graph at serve time.
3. **URLs are question-shaped.** `/context/<what>/<scope>?<filter>=<value>`. Not `/api/v1/cards/?status=wip&owner=silas`.
4. **Presentation optimized for consumption budget.** Flat, named, small. Ship an OpenAPI or schema artifact with every handler.
5. **Pull endpoints are citation-ready.** Response includes URL + timestamp in the envelope — an agent can cite "per /context/board/wip @ 09:20, silas has 2 WIP."
6. **Push envelope is a manifest + orientation, not a pre-synthesized blob.** Target <2KB.
7. **Interface design is reviewed.** `gate:interface` — named gate, separate from gate:arch/gate:code — will apply to new Context endpoints once defined (ADR TBD).

## Evaluation Lens (from chorus-overview)

Context passes the trinity (reliable / reused / valuable) only when:
- **Reliable.** Endpoints return correct data within predictable latency; common envelope shape stable; canonical stamps correct.
- **Reused.** Both agents AND UIs call Context endpoints (not two separate read paths); envelope injection and pull API share the same synthesis logic.
- **Valuable.** Claims in role responses trace back to Context calls; envelope content gets read (measurable via consumption metric). Today: not valuable by this bar — pulse injects but isn't consumed.

**Today's trinity score:** reliable (partial — Context works but inconsistent shape), reused (partial — pulse file read separately from API path, divergent), valuable (failing — consumption unverified, hallucinations observed). The redesign under #2234 brings all three up.

## Implementation Outline (scope of #2234)

1. **Endpoint inventory audit.** Classify every existing chorus-api endpoint: Context / Memory / Knowledge / None. Name redundant, dead, or attic endpoints for retirement.
2. **Design the minimum Context endpoint set.** `/context/board`, `/context/roles`, `/context/health`, `/context/spine`, `/context/nudges`, `/context/pulse` (envelope-shaped composite). Each with a declared response schema.
3. **Common envelope implementation.** One shared response-formatter library (Kade-owned, Services vertical) that every Context handler uses. Canonical metadata lookup via SPARQL at serve time.
4. **Data correctness: pick one.** Identify three worst sources of staleness in `/tmp/pulse-latest.json` today; fix one as part of #2234 to demonstrate the shape.
5. **Presentation: pick three.** Reshape three current JSON responses with worst consumption shape; ship redesign with schema.
6. **Push envelope reshape.** Replace pre-synthesized Pulse+Spine+Athena blob with manifest (endpoints) + orientation signal (timestamp, role, one-line health, one-line WIP count). Measure envelope byte-size before/after.
7. **Demonstration.** Live prompt where a role cites `/context/board/wip?role=X @ timestamp` in its response instead of inventing. Captured in #2234's demo brief.

## Out of Scope (for #2234)

- Memory service design — separate card, separate rhythm.
- Knowledge service design — separate card; dependencies on this one (canonical metadata contract) name the coupling.
- Full retirement of push-model envelope injection — #2234 reshapes it, doesn't remove it. Complete migration follows.
- `gate:interface` as a formal gate — flagged in the design rules above; ADR and implementation are follow-on.

## References

- `chorus-overview.md` — parent doc (refreshed 2026-04-19)
- `chorus-context-diagram-v2.html` — canonical visual
- `roles-service-design.md`, `quality-service-design.md` — sibling service designs, format source
- `platform/services/chorus-hooks/src/hooks/context_inject.rs` — current push-model injector
- `/tmp/pulse-latest.json` — current pulse snapshot format
- Staples ESB common-envelope pattern — design heritage (patent US9552400B2)
- #2234 — the card this design executes against
- #2231 — caching that composes with envelope shrink
- #2217 — ceremony audit surfaced Context consumption problem (OBS2 inflection)
- #2219 — pipeline-as-service design, parallel Framework-layer work
