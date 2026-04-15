# ADR-021: Ontology Enforcement Model

**Date**: 2026-04-15
**Status**: Accepted
**Deciders**: Silas (predicates), Wren (instances)
**References**: #1875 (gates graph), #2086 (skills graph), skill-lifecycle.html

## Context

The gates and skills subdomains needed to capture enforcement relationships — which gates are enforced by which binaries, how strongly, and what infrastructure they depend on. Without this, an agent can't answer "what breaks if chorus-hook-shim goes down?" or "which gates are actually enforced vs. documented?"

## Decision

Three new predicates in `urn:chorus:ontology`:

### chorus:dependsOn (ObjectProperty)
Runtime dependency. Subject requires object to be available. If object is down, subject is degraded or broken.

**Example:** `/pull` dependsOn `vikunja-environment` — the pull skill requires Vikunja to be up for card moves.

### chorus:enforcedBy (ObjectProperty)
Enforcement chain. A gate or constraint is enforced by a binary, hook, or skill. If the enforcer is down, the gate is unenforced.

**Example:** `tdd-gate` enforcedBy `chorus-hook-shim` — the Rust binary enforces test-first at edit time.

### chorus:enforcementLevel (DatatypeProperty)
How strongly a gate is enforced. Four values:
- **HARD-Rust** — compiled binary blocks the action. Cannot be bypassed without code change. (tdd-gate, icd-gate, write-scrubber, pair-gate)
- **HARD-skill** — skill script blocks the action. Can be bypassed by skipping the skill. (gate-product, gate-code, gate-quality, gate-arch, gate-ops, demo-gate, design-gate)
- **SOFT** — warns but proceeds. (documentation gates, convention checks)
- **NONE** — documented but unenforced. Constraint exists in ADR/decision but no automation prevents violation.

## SPOF Visibility

The enforcement model makes single points of failure visible:
- `chorus-hook-shim` is enforcedBy target for 17+ functions — if it crashes, 17 gates go dark
- `vikunja-environment` is dependsOn target for 7+ skills — if Vikunja is down, board ops fail
- `fuseki` is dependsOn target for domain context, SPARQL queries, completeness checks

These edges surface on the infrastructure domain page and in the dependency traversal API.

## Consequences

- Wren populates gate and skill instances with these predicates.
- Silas owns the predicate definitions in the ontology schema.
- SPOF analysis is now a graph query, not tribal knowledge.
