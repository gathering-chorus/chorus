# Brief: #1102 — Knowledge graph visualization

**From:** Wren
**To:** Kade
**Date:** 2026-03-06
**Priority:** P1

## Context
Jeff wants the same D3 interactive visualization you just built for the codebase (#842) applied to his full RDF knowledge graph. Same pattern, different data source — Fuseki instead of TTL files.

## What to build
- Endpoint: `/api/knowledge/graph` — query Fuseki SPARQL, return nodes + edges for D3
- Page: new view or extend existing mind map page
- Domain filtering: music, photos, stories, blog, people, practices, notes, self
- Progressive loading — domain-level first, expand on click (thousands of nodes total)
- Cross-domain edges: story → decision, track → artist → album, person → practice

## Key difference from codebase graph
Scale. The RDF graph has thousands of nodes (music alone is 2,700+ tracks). You need zoom levels and progressive loading — can't render the whole graph at once. Start with domain-level overview (like the mind map), click to expand.

## SPARQL pattern
```
curl -s 'http://localhost:3030/pods/query' -H 'Accept: text/csv' -G \
  --data-urlencode 'query=PREFIX jb: <https://jeffbridwell.com/ontology#> SELECT ...'
```
Dataset: `/pods`. Graph filter: `FILTER(STRSTARTS(STR(?g), "http://localhost:3000/pods/jeff/<domain>/"))`.

## Jeff's direction
"Would be interesting to do the same visualization on my graph" + "work on it too" — this is active, pull it next.

## AC
See card #1102 for full AC.
