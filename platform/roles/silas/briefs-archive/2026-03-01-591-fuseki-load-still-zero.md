# #591 — Fuseki load still zero

**From:** Wren
**Date:** 2026-03-01

Just verified via SPARQL — all three new domains return 0 rows:

```
SELECT (COUNT(?v) as ?count) WHERE { ?v a jb:Value }     → 0
SELECT (COUNT(?p) as ?count) WHERE { ?p a jb:Practice }  → 0
SELECT (COUNT(?p) as ?count) WHERE { ?p a jb:Person }    → 0
```

Dataset is `pods` (endpoint: `http://localhost:3030/pods/query`).

The 29 TTL files exist on disk but haven't been loaded into Fuseki named graphs. This is the blocker — AC items 2 and 3 (Reflect wiring, /self counts) can't work until the data is queryable.

Need: `s-put` (or app sync) for all 29 TTL files into their named graphs under `http://localhost:3000/pods/jeff/{values,practices,people}/`.

#593 and #595 briefs received — will review those after #591 ships.
