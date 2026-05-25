# Product / Subproduct Design — Template

Every Chorus product/subproduct design doc follows one **value-stream-aligned, model-aligned** shape. The doc's outline mirrors the Athena tree's containment edges: **value-stream step → product → domains → services**. Purpose first, implementation last.

## The shape (top-down)

1. **Step & purpose — what the product serves.** *Lead here.* Which value-stream step (Shaping · Designing · Directing · Building · Proving) the product serves, and the outcome it delivers there. Cite the canonical Athena entry (`iri` · atStep · status · owner · domains). This is the **PM hat**: why it exists, for whom.

2. **The product.** Its promise at that step — the value it delivers, its customers, and what it explicitly does *not* do (the sibling handoffs). Outcome-framed, not mechanics.

3. **Supporting domains.** How the product decomposes into domains (the `hasDomain` edges): the structure diagram + per-domain detail (promise, shipped, gaps, owner). This is the **Domain-Architect hat**: structure & fit.

4. **Domains → services.** How each domain composes into the services/verbs that run (the `hosts` edges). *Reference* the specific service-design docs + cards rather than re-asserting a service taxonomy — the services are the carded work, specified there.

Then supporting sections as needed: coherence claims, OWL/SHACL conformance, path-to-close, not-in-scope, appendices (card provenance, sources), references.

## Rules

- **Mirror the model.** The outline = the Athena containment edges (step → product → domain → service). Verify ownership/structure against the canonical tree (`chorus_tree_get`), not memory.
- **Purpose before implementation.** Lead with the value-stream step + outcome — not the promise, not the mechanics.
- **Don't re-assert what's specified elsewhere.** Services/verbs live in their own docs + cards; link them. Stop the structure at the level this doc owns; deeper levels are too deep.
- **Card refs → appendix.** Keep the body's claims clean; put `#NNNN` provenance in a Card-provenance appendix.
- **Mark TO-BE honestly.** If a level (e.g., service instances) isn't built/instantiated, say so — never imply it exists.
- **Names must be real or marked.** A name in a diagram implies a real thing; if it's notional/TO-BE, label it.
- **Whose work it is:** each role wears all four hats (PM · Domain Architect · Eng Lead · Ops Lead) for the products/domains it owns. The design doc is that role's PM + Domain-Architect output for its own domain — the PM-hat calls (what's worth asserting, does it trace to real use) answer to Jeff as Head of Product.

## Reference implementation

`werk-subproduct-design.html` (#3078, Kade) is the first doc built to this shape — use it as the worked example.
