# ADR-022: Graph Hygiene Rules

**Date**: 2026-04-15
**Status**: Accepted
**Deciders**: Jeff Bridwell, Silas, Wren, Kade
**References**: #2085 (graph hygiene), ADR-020 (product-vs-domain typing)

## Context

The ontology graph accumulated 70+ SubDomain/SubProduct nodes with duplicates, orphans, typos, and stale data from superseded graphs. A cleanup pass (#2085) collapsed 11 nodes and dropped the old `urn:gathering:ontology:chorus-product` graph (1,825 triples). These rules prevent the mess from recurring.

## Decision

### Rule 1: One node per concept
Never create two nodes for the same thing. Before creating a SubDomain, search for existing nodes with similar labels. Use `resolveSubdomainId()` in the API — if it finds a match, use that node.

### Rule 2: No orphans
Every SubDomain/SubProduct must have at least one parent edge — either `hasDomain` from a parent domain or `belongsTo` linking to a product. Orphaned nodes are invisible in the hierarchy and create islands in the viz.

### Rule 3: Instances, not subdomains
When multiple components serve one capability (like 5 discover-* scanners), create **one SubDomain** with the components as service/actor instances inside it. Don't create one SubDomain per component. The test: if they share an owner, step, and purpose, they're instances of one domain.

### Rule 4: Delete superseded graphs
When a new graph replaces an old one (e.g., `urn:chorus:ontology` replacing `urn:gathering:ontology:chorus-product`), DROP the old graph entirely. Don't leave it around — the viz reads all graphs and renders phantom nodes.

### Rule 5: Move data before deleting
When collapsing a node, always:
1. Snapshot the survivor's completeness percentage
2. Copy all instance data (actors, logs, services, pipelines, integrations, code files) from source to survivor
3. Transfer hasDomain edges from deleted parent to survivor
4. Delete the source node from both ontology and instances graphs
5. Verify survivor completeness >= pre-collapse baseline

### Rule 6: Fix tests after collapses
Any integration test referencing a deleted node URI must be updated to use the survivor's URI. Run the full test suite after every collapse pass.

### Rule 7: Resolve before write
Before creating a new node, run an ASK query (or use `resolveSubdomainId()`) to check if a matching node already exists. This prevents duplicate URIs at write time rather than requiring cleanup after the fact.

### Rule 8: Separate graphs for separate concerns
Use distinct named graphs for distinct data domains: `urn:chorus:ontology` for schema, `urn:chorus:instances` for domain instances, `urn:chorus:skills` for skills, `urn:chorus:gates` for gates, `urn:borg:instances` for infrastructure. Keeps queries fast (no cross-graph joins unless needed) and ownership clear (each graph has one writer).

## Consequences

- Graph node count is bounded by actual concepts, not historical accidents.
- New domain population follows the one-node rule from the start.
- The viz stays clean without periodic manual cleanup.
