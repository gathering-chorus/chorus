# ADR-027: Derived domain mappings live in the graph, not in code

**Status:** Accepted — 2026-04-28. Reviewed by Silas (architecture) and Kade (code impact); accepted by Jeff.
**Cards:** #2314 (Loom Principles API, Done), #2318 (Loom Decisions API, Later), #2516 (graphify alias map, Done). Blocks: #2554 (CatalogDocShape + onward), pending discover-code.ts + discover-pages migrations.
**Supersedes:** — (establishes the rule across a class of mappings that have been migrating one-by-one).
**Related:** ADR-025 (ontology vs instances graph separation — this ADR layers on that one).

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

**Derived domain mappings live in `urn:chorus:ontology`, not in code. Code reads them via SPARQL.**

A "derived domain mapping" means: any function in TypeScript / Rust / Python that takes a `SubDomain` (or sibling concept) and produces strings, paths, or aliases by *deriving* them from the SubDomain's id, label, or other attributes. Examples:

- `function buildAliasMap(subdomains)` returning `{ wordpress: 'blog-domain' }`
- `const DISCOVER_PAGES_GENERIC_BASES = ['index', 'home', ...]` keyed implicitly by domain
- `function aliasesForCode(domain)` returning `['service', 'svc', 'platform']`

When such a function exists, replace it with a predicate (`chorus:hasTestPathPrefix`, `chorus:hasGenericBase`, `chorus:hasCodeAlias`, etc.) declared on the `SubDomain` instance, and a SPARQL read at the consumption site.

Migration recipe (proven by #2314, #2318, #2516):
1. Declare the predicate in `urn:chorus:ontology`: `rdf:type rdf:Property + owl:DatatypeProperty/ObjectProperty`, `rdfs:domain`, `rdfs:range`, `rdfs:label`, `rdfs:comment`.
2. Run a one-time migration script (`scripts/migrate-<predicate>-to-graph.ts`) that computes the mapping from existing code (e.g., `deriveAliases` in #2516's migration) and INSERTs the triples.
3. Refactor the consumption site to read via a single SPARQL query with deterministic ordering (`ORDER BY ?sd`).
4. Delete the code-resident derivation (zero-hits grep on the const name).
5. Verify routing/output equivalence, **with explicit naming of any reroutes** (#2516 surfaced 9 corrective reroutes that an "identical output" assertion would have masked).

## Consequences

**Positive:**
- The graph becomes the reasoning surface for "what does this SubDomain own?" — agents (humans + LLMs) query one place.
- Adding a new SubDomain with its own mappings is a triple-write, not a code edit + PR + deploy.
- Deterministic ordering (`ORDER BY ?sd`) eliminates ordering bugs hidden by hash-map iteration.
- Graph-driven codegen (`project_graph_driven_codegen`) becomes possible: tooling can read predicates and generate forms, validators, or scaffolding.

**Negative / open:**
- **Onboarding gap**: declaring `hasTestPathPrefix` (or any derived mapping) is now a graph write, not a code edit. Domain owners need an affordance for declaring at SubDomain creation time. The right answer is **shape-driven form generation**, not bespoke per-predicate UI: a SHACL shape (per #2554's `chorus:CatalogDocShape` precedent) declares the contract; the form derives from the shape; the owner fills it in. UI-per-predicate is wrong leverage; UI-from-shape is right.
- **SHACL shape work compounds**: every predicate this ADR moves to the graph wants a corresponding shape constraint. #2554 is the precedent moment for that pattern, not a side-quest.
- **Migration scripts are one-shot**: keep them committed so the derivation logic is recoverable, but they don't run again. Treat them as audit trail, not infrastructure.
- **Discoverability of the rule**: a new contributor adding a `SPECIAL_ALIASES`-shaped const won't know about this ADR. Pre-commit lint hint (cite this ADR when a code-resident-derivation pattern is detected) lands as a follow-on, not in scope here.

## Pending applications

| # | Site | Predicate (proposed) | Card |
|---|------|---------------------|------|
| 4 | `discover-code.ts` generic bases | `chorus:hasCodeBase` | TBD — not yet filed |
| 5 | `DISCOVER_PAGES_GENERIC_BASES` | `chorus:hasPageBase` | TBD — not yet filed |

Both cards should cite this ADR. File when pulled, not before — so the ADR doesn't accrete card noise before the work is real.

## Citations

Kade owns the implementation citations; ask him for migration script paths and exact predicate names when filing the pending cards. Wren owns this ADR's narrative and will update on review feedback.
