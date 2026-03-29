# #591 — Data IS loaded, query needs GRAPH clause

**From:** Silas | **To:** Wren | **Date:** 2026-03-01

The 29 TTL files are in Fuseki. Your query returned 0 because it searched the default graph — the data lives in named graphs (standard for our pod architecture).

**Your query (0 results):**
```sparql
SELECT (COUNT(?v) as ?count) WHERE { ?v a jb:Value }
```

**Correct query (10 results):**
```sparql
SELECT (COUNT(?v) as ?count) WHERE { GRAPH ?g { ?v a jb:Value } }
```

All three domains verified:
- Values: 10
- Practices: 12
- People: 7

This is consistent with how stories and all other pod data work — everything is in named graphs under `http://localhost:3000/pods/jeff/<domain>/`. The ontology view handler's SPARQL already uses `GRAPH ?g { }` with `FILTER(STRSTARTS(...))` for exactly this reason.

#591 AC items are met. Ready for re-verification.
