# Brief: #1102 architecture response — Knowledge graph visualization

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-06
**Card:** #1102

## Overall assessment

The 3-level progressive loading is the right architecture. 37K graphs into a force layout is a non-starter — your instinct is correct. The decomposition into overview → drill-down → entity is sound and maps well to how Jeff actually explores data (big picture first, then drill).

## Answers

### 1. SPARQL query strategy

**Pre-aggregate for Level 1.** A single query that counts graphs and subjects per domain prefix:

```sparql
SELECT ?domain (COUNT(DISTINCT ?g) AS ?graphs) (COUNT(DISTINCT ?s) AS ?subjects)
WHERE {
  GRAPH ?g { ?s ?p ?o }
  BIND(REPLACE(STR(?g), "^http://localhost:3000/pods/jeff/([^/]+)/.*", "$1") AS ?domain)
}
GROUP BY ?domain
```

This should run in <2s even against the full dataset. Cache the result for 10 minutes — domain counts don't change within a session.

**Per-domain with LIMIT for Level 2.** Query within a single domain prefix, return the top N entities by triple count (or by a meaningful property like `jb:playCount` for music, `dcterms:created` for notes). Use `LIMIT 100` — that's plenty for D3 without sluggishness.

```sparql
SELECT ?entity ?label (COUNT(*) AS ?triples)
WHERE {
  GRAPH ?g { ?entity ?p ?o }
  FILTER(STRSTARTS(STR(?g), "http://localhost:3000/pods/jeff/music/"))
  OPTIONAL { ?entity rdfs:label ?label }
}
GROUP BY ?entity ?label
ORDER BY DESC(?triples)
LIMIT 100
```

**DESCRIBE + targeted CONSTRUCT for Level 3.** Single entity: `DESCRIBE <uri>` gives you everything about it. For cross-domain edges, see #4 below.

### 2. Caching layer

Per-domain TTLs — yes. Suggested tiers:

| Tier | TTL | Domains |
|------|-----|---------|
| Stable | 30min | music, people, socialposts, sexuality |
| Active | 5min | notes, stories, blog, intentions |
| Live | 60s | captures, codebase |

The overview (Level 1) gets its own 10min cache regardless of domain TTLs. Keep the same in-memory `Map<string, {data, expiry}>` pattern from the codebase graph — no need for Redis or external cache at this scale.

### 3. Sexuality domain

**Summary-only, no entity expansion.** Show the bubble at Level 1 with graph/triple counts. At Level 2, show chunk-level summaries (44 graphs — that's manageable as a list). Don't offer Level 3 entity drill-down until there's a specific UX need. The 700K-triples-per-chunk makes individual entity queries expensive and the data is structurally different (dense observation graphs, not discrete entities).

### 4. Cross-domain edges

The ontology defines explicit cross-domain properties. For a given entity, one-hop outbound gives you cross-domain links:

```sparql
SELECT ?predicate ?target ?targetDomain
WHERE {
  GRAPH ?g1 { <entity-uri> ?predicate ?target }
  GRAPH ?g2 { ?target ?p ?o }
  BIND(REPLACE(STR(?g1), "^http://localhost:3000/pods/jeff/([^/]+)/.*", "$1") AS ?sourceDomain)
  BIND(REPLACE(STR(?g2), "^http://localhost:3000/pods/jeff/([^/]+)/.*", "$1") AS ?targetDomain)
  FILTER(?sourceDomain != ?targetDomain)
}
LIMIT 50
```

Key cross-domain properties to look for: `jb:byArtist`, `jb:inAlbum`, `jb:mentions`, `dcterms:references`, `schema:about`. The search index does text-based cross-domain matching — this is the graph-native version of the same concept.

**For Level 1 overview**, add inter-domain edge counts as a bonus query: how many triples connect music↔people, stories↔people, etc. This makes the domain bubbles connected, not isolated circles.

### 5. Endpoint pattern

`/api/knowledge/*` is clean. One tweak:

| Endpoint | Purpose |
|----------|---------|
| `GET /api/knowledge/overview` | Level 1 — domain bubbles + inter-domain edges |
| `GET /api/knowledge/domain/:name?limit=100` | Level 2 — entities within domain |
| `GET /api/knowledge/entity/:domain/*` | Level 3 — single entity + cross-domain |

I'd use `/overview` instead of `/graph` to avoid confusion with `/api/codebase/graph`. The `/:domain/*` pattern on entity lets you pass the full URI path naturally.

## One more thing

Consider a `?format=d3` query param that returns nodes/edges pre-shaped for D3 (same pattern as the codebase graph API). Keeps the frontend simple — just fetch and render, no transformation step.
