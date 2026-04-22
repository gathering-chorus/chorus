# Brief → Kade — #2348 Wave 2: practices endpoint + three-layer Loom render

**From:** Wren  
**Date:** 2026-04-21  
**Card:** #2348 (Map Chorus practices to loom-principles + loom-policies)  
**Wave 1 status:** Done. Ontology populated in Fuseki + chorus.ttl. 40 practices, 84 expresses edges, 25 operationalizes edges, 0 orphans, 1 abstract principle.

## What I need

**AC5:** Loom page renders the three-layer view. Clicking a principle shows (principles → policies that enforce → practices that express). Clicking a practice shows the upstream chain.

## Scope (two pieces)

### 1. `/api/loom/practices` handler

Mirror the shape of `platform/api/src/handlers/loom-principles.ts` and `loom-policies.ts`. Handler + SPARQL file pair.

SPARQL shape (similar to loom-policies.sparql):
```sparql
PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?practice ?label ?comment ?principle ?principleLabel ?policy ?policyLabel ?ceremonyRisk WHERE {
  GRAPH <urn:chorus:ontology> {
    ?practice a chorus:Practice .
    OPTIONAL { ?practice rdfs:label ?label }
    OPTIONAL { ?practice rdfs:comment ?comment }
    OPTIONAL { ?practice chorus:ceremonyRisk ?ceremonyRisk }
    OPTIONAL { ?practice chorus:expresses ?principle . OPTIONAL { ?principle rdfs:label ?principleLabel } }
    OPTIONAL { ?practice chorus:operationalizes ?policy . OPTIONAL { ?policy rdfs:label ?policyLabel } }
  }
}
```

Response shape — fold to `{ practices: [ { id, label, comment, expresses: [{id,label}], operationalizes: [{id,label}], ceremonyRisk } ] }`. Same envelope as loom-principles/loom-policies (`_meta`, `data`).

### 2. Three-layer render on the Loom page

File: `jeff-bridwell-personal-site/public/gathering-docs/domain-loom.html` (and whatever JS it loads).

Behavior:
- Top-level view shows all three layers side by side (principles | policies | practices) with edge-counts.
- Clicking a **principle** highlights: (a) the policies that `enforces` it, (b) the practices that `express` it.
- Clicking a **policy** highlights: (a) its principle(s) via `enforces`, (b) the practices that `operationalize` it.
- Clicking a **practice** highlights: (a) principle(s) via `expresses`, (b) policy (if any) via `operationalizes`.
- Abstract principles (`chorus:abstract true`) rendered with a visual flag ("abstract — no practice enacts this"). Currently one: `principle-use-edges-and-value-the-marginal`.
- Ceremony-risk practices (`chorus:ceremonyRisk true`) rendered with a visual flag ("ceremony risk"). Currently none, but the rendering needs to handle the case.

## Constraints

- Same graph: `urn:chorus:ontology`. No new TTL files.
- Same CORS stopgap as loom-principles/loom-policies (the #2041 debt tag).
- Follow DEC-1674 TDD: write handler tests first (mirror `tests/handlers/loom-principles.test.ts`).

## Acceptance

Wave 2 passes when:
1. `GET /api/loom/practices` returns 40 practices with full edge data.
2. `/gathering-docs/domain-loom.html` renders the three-layer view with click-through behavior.
3. Abstract/ceremony-risk flags visually distinguishable.
4. All handler tests green.

Wave 1 (my side) is shippable independently — the ontology is complete and queryable. Wave 2 is the user-surface layer. I'll /acp #2348 when both waves demo.

## Context memory

- Memory: `project_ontology_as_reasoning_surface` — the graph is the agent reasoning surface, metadata before code.
- Memory: `project_assemblage_model` — principles, policies, practices shape each other.
- Card description has the full AC + "rule of thumb" framing.

Nudge me when the handler lands and I'll sanity-check the fold shape before you move to the render.
