# ADR-027: Derived domain mappings live in the graph, not in code

**Status:** Accepted — 2026-04-28. Reviewed by Silas (architecture) and Kade (code impact); accepted by Jeff.
**Cards:** #2314 (Loom Principles API, Done), #2318 (Loom Decisions API, Later), #2516 (graphify alias map, Done). Blocks: #2554 (CatalogDocShape + onward), pending discover-code.ts + discover-pages migrations.
**Supersedes:** — (establishes the rule across a class of mappings that have been migrating one-by-one).
**Related:**
- ADR-025 (ontology vs instances graph separation — this ADR is the operational expression for one class of mapping).
- Loom principles: `ontology-is-architecture` and `cross-project-coherence` (this ADR operationalizes both for derived SubDomain mappings).

## Scope

This ADR applies to derived mappings on `chorus:SubDomain` instances. Today only SubDomain has derived mappings of the kind discussed here; if Person / Role / Decision / similar instances later acquire derived mappings of the same shape, the rule generalizes naturally. Until then, "domain mappings" means SubDomain mappings.

## Context

Three independent migrations in this repo have followed the same shape:

1. **Principles** (#2314, Done): `chorus:hasPrinciple` predicate declared in `urn:chorus:ontology`; loom-principles read path now SPARQL.
2. **Decisions** (#2318, in progress): `chorus:hasDecision` predicate, same pattern; markdown decision files retiring.
3. **Test alias mapping** (#2516, Done 2026-04-28): `chorus:hasTestPathPrefix` predicate added; `SPECIAL_ALIASES` const + `buildAliasMap` auto-derivation removed from `platform/api/src/discover-tests.ts`; reads via SPARQL with `ORDER BY ?sd`. Side benefit: 9 tests previously misrouting under non-deterministic Fuseki order now route correctly.

Two more sites have the same shape but haven't migrated yet:

4. **Code discovery generic bases** — `platform/api/src/discover-code.ts` (similar derivation logic, code-resident; cited #2516 out-of-scope).
5. **Page discovery** — `DISCOVER_PAGES_GENERIC_BASES` in `platform/api/src/server.ts` (or sibling). Same code-resident derivation.

Three landed cases is a streak. Five points define the line — and the line is at the architectural seam between code-resident derivation and graph-resident declaration. We should name the rule before two more accretions land in code without it.

The deeper principle this connects to: the ground-truth-of-a-domain-mapping is part of the **canonical model**, not part of the **implementation that consumes the model**. When alias mapping lived in TS (`SPECIAL_ALIASES` + auto-derivation), the question "which subdomain owns `wordpress` tests?" had to be answered by reading a 50-line function. After #2516, the same question is one SPARQL `SELECT`. The graph is the agent's reasoning surface (per `project_ontology_as_reasoning_surface`); code-resident derivation hides reasoning from agents.

## Decision

**Derived domain mappings live in `urn:chorus:ontology` (predicate declaration) and `urn:chorus:instances` (per-SubDomain triples), not in code. Code reads them via SPARQL.** Two-graph operation per migration, per ADR-025.

A "derived domain mapping" means: any function or const that produces strings, paths, or aliases **by computing them from a SubDomain's id, label, or other attributes** — i.e., the values are part of the canonical model the agent reasons over. Examples:

- `function buildAliasMap(subdomains)` returning `{ wordpress: 'blog-domain' }`
- `const DISCOVER_PAGES_GENERIC_BASES = ['index', 'home', ...]` keyed implicitly by domain
- `function aliasesForCode(domain)` returning `['service', 'svc', 'platform']`

This rule does **NOT** apply to:
- Presentation-only constants (CSS palettes, format strings, copy) — not part of the canonical model.
- Domain-adjacent literals that happen to live near domain code but aren't computed from SubDomain attributes.
- One-shot derivations consumed by exactly one runtime caller and never queried by anyone else.

The test for "is this derived?" — does the value answer a question about a SubDomain that another agent (human or LLM) might ask the graph? If yes, graph. If no, code is fine.

When such a derivation exists, replace it with a predicate (`chorus:hasTestPathPrefix`, `chorus:hasCodePathPrefix`, etc.) declared on the `SubDomain` instance, and a SPARQL read at the consumption site.

Migration recipe (proven by #2314, #2318, #2516):

1. **Declare the predicate** in `urn:chorus:ontology`: `rdf:type rdf:Property + owl:DatatypeProperty/ObjectProperty`, `rdfs:domain chorus:SubDomain`, `rdfs:range xsd:string` (or appropriate type), `rdfs:label`, `rdfs:comment`. Predicate lives in the **ontology** graph; the triples it enables live on instances in `urn:chorus:instances` (per ADR-025 separation).
2. **Migration script — idempotent.** `scripts/migrate-<predicate>-to-graph.ts` computes the mapping from existing code (e.g., `deriveAliases` in #2516's migration) and lands triples in `urn:chorus:instances`. **Idempotent shape: DELETE WHERE matching predicate triples in target graph, then INSERT DATA.** Without the explicit DELETE, re-running silently doubles the triples — proven failure mode the next migrator should not have to rediscover.
3. **Refactor the consumption site** to read via a single SPARQL query. **Use `ORDER BY` on the SubDomain identifier** for deterministic resolution of multi-binding collisions. #2516's `properties/property` collision is the precedent: when the alias `properties` matched both `property-domain` (real) and an unrelated SubDomain via overlapping prefix, hash-map iteration order picked one or the other depending on the run. `ORDER BY ?sd` resolves the same collision the same way every time. A predicate without collision exposure today may acquire one tomorrow when a new SubDomain lands — the ordering is cheap; non-deterministic order hides routing bugs across runs and across machines.
4. **Delete the code-resident derivation** (zero-hits grep on the const name).
5. **Verify routing/output equivalence**, **with explicit naming of any reroutes** (#2516 surfaced 9 corrective reroutes that an "identical output" assertion would have masked).

## Consequences

**Positive:**
- The graph becomes the reasoning surface for "what does this SubDomain own?" — agents (humans + LLMs) query one place.
- Adding a new SubDomain with its own mappings is a triple-write, not a code edit + PR + deploy.
- Deterministic ordering eliminates ordering bugs hidden by hash-map iteration.
- Graph-driven codegen (`project_graph_driven_codegen`) becomes possible: tooling can read predicates and generate forms, validators, or scaffolding.

**Negative / open:**
- **Onboarding gap**: declaring `hasTestPathPrefix` (or any derived mapping) is now a graph write, not a code edit. Domain owners need an affordance for declaring at SubDomain creation time. The right answer is **shape-driven form generation**, not bespoke per-predicate UI: a SHACL shape (per #2554's `chorus:CatalogDocShape` precedent) declares the contract; the form derives from the shape; the owner fills it in. UI-per-predicate is wrong leverage; UI-from-shape is right.
- **SHACL shape work compounds**: every predicate this ADR moves to the graph wants a corresponding shape constraint. #2554 is the precedent moment for that pattern, not a side-quest.
- **Migration scripts are one-shot**: keep them committed so the derivation logic is recoverable, but they don't run again. Treat them as audit trail, not infrastructure.
- **Discoverability of the rule**: a new contributor adding a `SPECIAL_ALIASES`-shaped const won't know about this ADR. Pre-commit lint hint (cite this ADR when a code-resident-derivation pattern is detected) lands as a follow-on, not in scope here.

## Pending applications

| # | Site | Predicate (proposed) | Card |
|---|------|---------------------|------|
| 4 | `discover-code.ts` generic bases | `chorus:hasCodePathPrefix` | TBD — not yet filed |
| 5 | `DISCOVER_PAGES_GENERIC_BASES` | `chorus:hasPagePathPrefix` | TBD — not yet filed |

Naming follows `chorus:hasTestPathPrefix` (#2516) — `hasXPathPrefix` for any datatype-property mapping that matches a token within a path. Strict accuracy is "matches anywhere via includes()" not "prefix only," but consistency with the landed predicate wins; a future renaming pass can clean both at once if "prefix" becomes misleading.

Both cards should cite this ADR. File when pulled, not before — so the ADR doesn't accrete card noise before the work is real.

## Citations

Kade owns the implementation citations; ask him for migration script paths and exact predicate names when filing the pending cards. Wren owns this ADR's narrative and will update on review feedback.

**Review history:**
- 2026-04-28 (Wren) — initial draft.
- 2026-04-28 (Silas) — five tightening notes applied: explicit "derived" criterion + non-example, ordering rationale, ADR-025 two-graph framing, scope clarified to SubDomain, loom-principles citations.
- 2026-04-28 (Kade) — code-impact pass: idempotent migration step (DELETE WHERE → INSERT DATA), collision-resolution rationale (`properties/property` precedent), predicate naming aligned to `hasXPathPrefix` symmetry, drift between line-33 example and pending-table fixed.
- 2026-04-28 (Jeff) — Accepted.
