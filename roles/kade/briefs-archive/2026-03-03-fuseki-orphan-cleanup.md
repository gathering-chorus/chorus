# Brief: Add orphan graph cleanup to fuseki-sync

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-03
**Priority:** P2

## Context

Card #536 deleted stale `media/` TTL files from disk but never dropped the corresponding Fuseki graphs. The `fuseki-sync.service.ts` `fullSyncAll()` is additive only — it loads graphs from TTL files but never checks for graphs in Fuseki that no longer have source files.

This left ~13.5M orphan triples in 5 `media/` graphs. I'm dropping them manually via Graph Store Protocol DELETE today, but the systemic gap remains.

## Root Cause

`fuseki-sync` workflow:
1. Scan `data/pods/jeff/<domain>/` for TTL files
2. For each TTL file, PUT to Fuseki graph
3. **No step 3** — no check for Fuseki graphs without corresponding TTL files

## Recommendation

Add an orphan detection step to `fullSyncAll()`:

1. After all domains sync, query Fuseki for all graph URIs: `SELECT DISTINCT ?g WHERE { GRAPH ?g { ?s ?p ?o } }`
2. For each graph URI, derive the expected filesystem path (reverse the URI → path mapping)
3. If no TTL file exists on disk → drop the graph via `dropGraph()` (already exists in `sparql.service.ts`)
4. Log dropped orphans as warnings

### Safety constraints
- Only drop graphs under the `http://localhost:3000/pods/jeff/` prefix (never system graphs)
- Dry-run mode first: log what would be dropped without acting
- Add a `--cleanup` flag to `fullSyncAll()` so it's opt-in initially

## Files to modify
- `src/services/fuseki-sync.service.ts` — add orphan detection after sync loop
- `src/services/sparql.service.ts` — `dropGraph()` already exists, reuse it

## Acceptance Criteria
- `fullSyncAll({ cleanup: true })` detects and drops orphan graphs
- Dry-run output shows which graphs would be dropped
- Existing sync behavior unchanged without the flag
- Test coverage for orphan detection logic
