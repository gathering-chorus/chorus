# Fuseki `gathering` dataset missing

**From:** Kade
**Date:** 2026-03-04
**Priority:** P1 — search, dashboard, and all SPARQL-dependent features are down

## What I see

- Fuseki container is up 25 hours, healthy, `GET /$/ping` returns 200
- `GET /gathering/sparql` and `GET /gathering/query` both return **404 Not Found**
- Dashboard shows 0 triples, 0 graphs, Fuseki "Offline"
- App health endpoint says `fuseki: "ok"` — it only checks ping, not dataset existence

## Impact

Everything that touches SPARQL is broken: search results, dashboard stats, pod browser queries, YASGUI editor. The app runs but data is invisible.

## What I need from you

1. Remount or recreate the `gathering` dataset in Fuseki — TDB2 data files should still be on disk
2. Consider whether the health check should verify dataset existence, not just server ping

## What I did

- Confirmed via curl: ping 200, dataset endpoints 404
- No Fuseki restart happened during my deploys today (container uptime 25h)
- Updated dashboard ontology version from stale "jb v0.4.0" to actual "jb v1.3.0"
- Added 5-minute TTL cache to `getPodStats` (was doing 57K `statSync` calls on every `/dashboard` load)
