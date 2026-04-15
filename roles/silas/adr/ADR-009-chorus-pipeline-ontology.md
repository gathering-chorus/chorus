# ADR-009: Chorus Pipeline Ontology

**Date**: 2026-02-19
**Status**: Deferred — accepted but not shipped. Pipeline ontology concept partially realized via Athena subdomain graph and skill/gate enforcement model (ADR-021)
**Decider**: Wren (product), Silas (architecture)
**References**: DEC-019 (Chorus naming), DEC-023 (Chorus pipeline as operating model), US9552400B2 (Bridwell patent — RDF/OWL + SPARQL + workflow gates)

## Context

Chorus (DEC-019) is the team coordination product — one human directing multiple AI agents through role-based personas, async handoffs, and instrumented fitness functions. The pipeline (Directing → Designing → Building → Proving) emerged as our operating model (DEC-023), but it exists only in docs and heads.

To make the pipeline testable, instrumentable, and eventually self-reporting, we need it modeled as RDF. This follows the same pattern Jeff applied at Staples (US9552400B2): ontology as execution substrate. Gates are SPARQL queries, not code.

## Decision

Model the Chorus pipeline as a 6-layer RDF ontology (`chorus.ttl`), extending the existing `building.ttl` team protocol ontology.

### Layer 1: Pipeline Stages
Four stages: Directing → Designing → Building → Proving. Each stage has:
- Owner (who's responsible)
- Entry conditions (what must be true to enter)
- Exit artifact (what gets produced)

### Layer 2: Gates
Quality gates between stages. Each gate has:
- A gatekeeper (role responsible for evaluation)
- Pass/fail criteria (eventually SPARQL queries)
- BounceBack mechanism — failed work returns to a prior stage with feedback
- **Non-linear bounces**: work can bounce backward multiple stages (e.g., Building → Directing if scope is wrong)

### Layer 3: Roles + Artifacts
Maps building.ttl roles to pipeline stages. Artifacts (briefs, ADRs, cards, commits) are typed and linked to stages and gates.
- **Artifact versioning**: `chorus:version` and `chorus:supersedes` track revisions

### Layer 4: Sessions + Flow
WorkItems flow through stages. Sessions record who worked on what, when. Gate transitions create an audit trail.

### Layer 5: Fitness Functions + Trust
Trust is emergent from gate passage rate: `gatesPassed / totalGateAttempts`. Override tracking keeps it honest. Bounce rate per gate identifies pipeline weaknesses.

### Layer 6: Execution Engine (future)
Phase 1: board.sh wrapper with gate-check. Phase 2: RDF state tracking. Phase 3: live Grafana dashboard. Only Phase 1 is approved.

### Known Limitations
- **Parallel work items**: WIP limit is 2. Model supports this but v0.1.0 doesn't instrument contention. Planned for v0.2.0.
- **Proving → Directing feedback**: Completed pipeline cycles should generate Signals visible in the next Directing stage. Modeled via `chorus:feedsInto` but not yet instrumented.

## Prior Art

Jeff Bridwell's US Patent 9,552,400 B2 defines an RDF/OWL ontology with SPARQL-driven workflow gates at enterprise scale (Staples). Core insight: the ontology IS the execution substrate — change a gate by changing a query, not by redeploying code. Chorus applies this pattern to human+AI team coordination.

## Consequences

**Positive:**
- Pipeline becomes testable (SPARQL queries against real data)
- Gate passage creates audit trail
- Trust is measurable, not assumed
- Extends naturally to automated gate checking

**Negative:**
- Adds ontology maintenance burden (another .ttl file to keep current)
- RDF state tracking (Phase 2) needs Fuseki integration work

---

-- Silas
