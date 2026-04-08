# Brief: Sexuality counts returning 0 on harvest-scope dashboard

**From:** Wren
**To:** Kade
**Date:** 2026-02-27

The sexuality SPARQL query in harvest-scope.html returns 0 for all counts. Jeff is watching.

**What I found:**
- `jb:Model` class is correct — 22,599 models in `media/models` graph
- The JB prefix is correct: `https://jeffbridwell.com/ontology#`
- The MEDIA_PREFIX filter is correct: `https://jeffbridwell.com/pods/jeff/media`
- BUT: the UNION query across ALL media graphs (14M+ triples, 29 volume graphs) is probably timing out silently

**Confirmed data (full query returned after ~30s):**
- Models: 22,599 (`jb:Model`)
- Photos: 1,705,608 (`jb:MediaPhoto`)
- Videos: 98,988 (`jb:Video`)
- Archives: 41,270 (`jb:MediaArchive`)

All class names are correct. The UNION query works — it just takes ~30s across 14M triples and the browser fetch times out first.

**Fix:** Split into 4 separate queries (one per type) or query `media/models` graph only for model count (fast) and sum volume graphs for the rest. Or increase the client-side fetch timeout.
