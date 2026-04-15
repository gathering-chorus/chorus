# ADR-020: Product vs Domain Typing in Ontology

**Date**: 2026-04-15
**Status**: Accepted
**Deciders**: Jeff Bridwell, Wren, Silas
**References**: DEC-1786 (graph-lens architecture), #2085 (graph hygiene)

## Context

The ontology graph had duplicate nodes — the same concept appearing as both a SubProduct and a SubDomain. Observability existed as both `observability-product` (SubProduct) and `observability-domain` (SubDomain). Five discover-* scanners were individual SubProducts when they're components of one capability. The viz showed phantom duplicates and agents couldn't reason about which node was canonical.

## Decision

**One rule:** If Jeff uses it as a surface (UI, CLI, browser page), it's a **SubProduct**. If it's a capability area, it's a **SubDomain**.

### SubProducts (user surfaces)
- Athena — domain detail pages, completeness UI
- Cards — board CLI, Vikunja UI
- Clearing — browser chat
- Werk — session dashboard
- Pulse — health dashboard
- Loom — policy/practice browser

### SubDomains (capability areas)
Everything else: Infrastructure, Observability, Deploys, Security, Convergence, Gates, Skills, Roles, Heralds, etc.

### Structural Rules
- **One node per concept.** Never create both a SubProduct and SubDomain for the same thing.
- **No orphans.** Every SubDomain has at least one parent edge (hasDomain or belongsTo).
- **Heralds pattern.** When multiple scanners/agents serve one capability, create one SubDomain with the scanners as service instances inside it — not one SubDomain per scanner.
- **Products own lenses, not data** (DEC-1786). A product's hasDomain edges define its lens into the graph. The graph data belongs to the domain, not the product.

## Consequences

- Graph hygiene (#2085) collapsed 11 nodes following this rule.
- New subdomains must pass the "does Jeff use it as a surface?" test before choosing SubProduct.
- The viz shows clean, non-duplicate labels.
