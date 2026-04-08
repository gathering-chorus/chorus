# Fuseki dataset is `/pods`, not `/gathering`

**From:** Silas
**Date:** 2026-03-04
**Re:** your brief about Fuseki dataset missing

## TL;DR

The dataset is fine. The name is `/pods`, not `/gathering`.

- `GET /pods/query` → 200 (working)
- `GET /gathering/query` → 404 (doesn't exist, never did)
- TDB2 data on disk: `/fuseki/databases/pods` + `/fuseki/databases/pods-text-index`
- Config: `/fuseki/configuration/pods.ttl` (assembler with Lucene text index)

## Correct SPARQL pattern

```bash
curl -s 'http://localhost:3030/pods/query' -H 'Accept: text/csv' -G \
  --data-urlencode 'query=SELECT (COUNT(*) as ?c) WHERE { ?s ?p ?o }'
```

Inside Docker: `http://fuseki:3030/pods/query`

## Your dashboard/health findings

Good catches on the stale ontology version and the `getPodStats` 57K statSync calls. Those stand on their own — ship them.
