# ADR-017: Enrichment Architecture — Separate Fact from Inference

**Status:** Accepted
**Date:** 2026-03-12
**Context:** Google Photos (157 files, ~628GB), Google Docs (330K), and future harvests will create 20-50M+ new triples. Connecting this data (photos → places, docs → people, events across domains) is the next challenge. The risk is polluting the graph with speculative edges that are wrong or stale, then losing track of which connections are ground truth vs. inferred.

## Decision

**Two-pass architecture: harvest fast, enrich later. Enrichment writes to dedicated named graphs, never into harvest graphs.**

### Graph Layer Model

**Layer 1 — Harvest (immutable facts)**
What the source system told us. Never modified by enrichment.
- `jb:Photo` with title, dateTaken, GPS coords, album, source file
- `jb:Document` with title, created, modified, folder, mimeType
- `jb:Album`, `jb:Artist`, `jb:Track` — same as today

**Layer 2 — Enrichment (inferred edges, stored separately)**
Connections created by enrichment rules. Each edge carries provenance:

```turtle
GRAPH <pods/jeff/enrichment/geo-2026-03-12> {
  <photo/123> jb:locatedAt <place/home> .
  <photo/123> jb:enrichedBy "geo-reverse-v1" .
  <photo/123> jb:enrichedAt "2026-03-12T14:00:00Z" .
}
```

### Why Separate Graphs

- **Retractable** — drop a whole enrichment graph to undo a bad run. No surgery on harvest data.
- **Re-runnable** — delete the old enrichment graph, run the improved rule, write a new one. Clean swap.
- **Auditable** — SPARQL can distinguish harvest facts (Layer 1) from inferred connections (Layer 2) by graph name.
- **Iteration-safe** — bad enrichment gets dropped entirely, not patched over. Jeff can iterate on rules without cluttering the graph.

### Enrichment Passes

Each pass is independent, idempotent, and writes to its own named graph.

| Pass | Input | Output Edge | Method |
|------|-------|------------|--------|
| Geo → Place | GPS coords on photos | `jb:locatedAt` → `jb:Place` | Reverse geocode, cluster to known places |
| Date → Event | dateTaken clusters | `jb:partOf` → `jb:Event` | Photos within same day + same place = event |
| Person mention | Doc text, photo metadata | `jb:mentions` → `jb:Person` | Name matching against People graph |
| Doc → Topic | Doc title + folder path | `jb:relatedTo` → `jb:Topic` | Folder hierarchy as taxonomy seed |
| Cross-domain | Date + Person + Place overlap | `jb:relatedTo` between domains | Composite connections across harvests |

### Review Gate (Staging → Production)

1. **Run** — script produces TTL with enrichment graph
2. **Preview** — load to staging graph (`enrichment/staging/<pass>-<version>`), query, spot-check
3. **Promote** — rename to production graph name, app picks it up
4. **Or drop** — delete staging graph, iterate, try again

### Ontology Additions

- `jb:enrichedBy` — rule/script identifier (string)
- `jb:enrichedAt` — timestamp (xsd:dateTime)
- `jb:confidence` — optional, for fuzzy matches like name resolution (xsd:decimal, 0-1)
- Graph naming convention: `pods/jeff/enrichment/<pass>-<date>`

Existing ontology shapes (`jb:Photo`, `jb:Person`, `jb:Place`) do not change. Enrichment adds edges between existing nodes.

## Consequences

- Harvest pipeline stays simple and fast — no enrichment logic in the ingest path
- Enrichment rules can evolve independently — new rules don't require re-harvesting
- Bad enrichment runs are cheap to undo — drop the graph, try again
- SPARQL queries that need only facts hit Layer 1; queries that want connections join both layers
- Fuseki memory impact: enrichment graphs add edges but not new nodes, so triple count growth is proportional to connection density, not data volume
