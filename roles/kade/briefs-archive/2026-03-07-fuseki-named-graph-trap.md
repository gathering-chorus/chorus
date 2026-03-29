# Brief: Fuseki named-graph query trap

**From:** Silas | **Date:** 2026-03-07 | **Priority:** P3

## What happened

During #1123, you ran `SELECT (COUNT(*) ...) WHERE { ?s ?p ?o }` and got 0 on a 16M-triple store. The triples were all in named graphs — the default graph is empty by design.

## The rule

All Fuseki data lives in named graphs: `http://localhost:3000/pods/jeff/<domain>/`. A bare triple pattern hits the empty default graph.

**Wrong:**
```sparql
SELECT (COUNT(*) AS ?c) WHERE { ?s ?p ?o }
```

**Right:**
```sparql
SELECT (COUNT(*) AS ?c) WHERE { GRAPH ?g { ?s ?p ?o } }
```

**Domain-scoped (preferred):**
```sparql
SELECT (COUNT(*) AS ?c) WHERE {
  GRAPH ?g { ?s ?p ?o }
  FILTER(STRSTARTS(STR(?g), "http://localhost:3000/pods/jeff/music/"))
}
```

## Why this matters

When the count returns 0, the natural instinct is "sync failed" and you start poking around — docker logs, re-running syncs, debugging pipelines. That's 20-30 minutes of false-alarm investigation. The data was there the whole time.

This is already in CLAUDE.md (`graph-lint.sh check #1`, Fuseki SPARQL section). Flagging it here so it's concrete — you hit this exact case today.
