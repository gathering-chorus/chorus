# Brief: Add Implementation Metadata Section to ICDs

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-20
**Re:** ICD as CMDB — close the loop between schema and implementation

## Context

Jeff wants implementation details captured in the ICD so it's the single source of truth for what a domain IS — not just field names, but how data flows. This connects to #1553 (domain topology graph): if ICDs carry implementation metadata, the topology can be generated from ICDs rather than built separately.

Some implementation info is already scattered in the SEMANTIC_MAPPER (Pipeline Status sections, harvester references, Non-Functionals). This brief formalizes the structure so every domain has it consistently.

## Proposed Section: Implementation Contract

Add to each domain body in the ICD, after Non-Functionals:

```html
<!-- Implementation Contract -->
<div class="icd-section">
  <div class="icd-section-header">Implementation Contract</div>
  <div class="icd-section-content">
    <table class="icd-table">
      <tr><th>Aspect</th><th>Value</th></tr>
      <tr><td>Harvester</td><td><code>StoryHarvestService</code> / <code>harvest-stories.sh</code></td></tr>
      <tr><td>Source Path</td><td><code>data/pods/jeff/stories/items/*.ttl</code></td></tr>
      <tr><td>Fuseki Graph Base</td><td><code>urn:gathering:stories/items/</code></td></tr>
      <tr><td>API Endpoints</td><td><code>GET /api/stories</code>, <code>GET /api/stories/:id</code></td></tr>
      <tr><td>Page Routes</td><td><code>/stories</code>, <code>/stories/:id</code></td></tr>
      <tr><td>ICD Instance</td><td><code>icd-instance-stories.ttl</code></td></tr>
      <tr><td>Validation Gate</td><td><code>validateFromICD('stories')</code></td></tr>
      <tr><td>Sync Script</td><td><code>harvest-sync-fuseki.sh stories/items</code></td></tr>
      <tr><td>Manifest</td><td><code>data/harvest/manifests/stories.json</code></td></tr>
    </table>
  </div>
</div>
```

## Fields per domain

| Field | Description | Example |
|-------|-------------|---------|
| Harvester | Service class or script that extracts data | `NotesHarvestService` |
| Source Path | Where raw/TTL data lives on disk | `data/pods/jeff/notes/items/` |
| Fuseki Graph Base | Named graph pattern in Fuseki | `urn:gathering:notes/items/` |
| API Endpoints | REST routes that serve this domain | `GET /api/notes`, `POST /api/notes` |
| Page Routes | Express routes that render this domain | `/notes` |
| ICD Instance | The RDF ICD definition file | `icd-instance-notes.ttl` |
| Validation Gate | Function/hook that gates writes | `validateFromICD('notes')` |
| Sync Script | How data gets to Fuseki | `harvest-sync-fuseki.sh notes/items` |
| Manifest | Harvest manifest tracking pipeline state | `data/harvest/manifests/notes.json` |

## Why this matters

1. **CMDB from ICDs** — #1553 topology graph can be generated from this metadata instead of hand-built
2. **Onboarding** — new session (or new role) reads the ICD and knows everything: schema, mappings, AND plumbing
3. **Audit** — "what touches this domain?" is one query, not a codebase search
4. **Jeff's Staples pattern** — the ICD at Staples had transport and transform details, not just field names

## Also in RDF

This metadata should also exist as RDF triples on the ICD ontology instances in Fuseki, so the topology graph (#1553) can be SPARQL-queried:

```turtle
gathering:domain-stories a gathering:Domain ;
    gathering:harvester "StoryHarvestService" ;
    gathering:sourcePath "data/pods/jeff/stories/items/" ;
    gathering:fusekiGraphBase "urn:gathering:stories/items/" ;
    gathering:pageRoute "/stories" ;
    gathering:icdInstance "icd-instance-stories.ttl" .
```

## Action

1. Define the Implementation Contract section structure (HTML + RDF)
2. Populate for the 4 active domains: Notes, Stories, Photos, People
3. Wire into ICD API so `/api/icd/domains/:id` returns implementation metadata
4. Update the Convergence page to render this section

This is Chunk B foundation work — the ICD governs everything, including the plumbing.
