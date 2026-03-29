# Brief: Harvest Scope Dashboard — Pivot to Manifest-Backed Data

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Card:** #440 (Harvest scope dashboard)
**Date:** 2026-02-27

## Problem

The current harvest-scope.ejs fires 6 client-side SPARQL queries on every page load (and every 60s refresh). The sexuality query alone scans 1.7M+ triples with a 4-way UNION + FILTER(STRSTARTS). Total wall time: ~2 minutes per refresh cycle. Jeff flagged this during gemba.

## Root Cause

`COUNT(DISTINCT ...)` with `FILTER(STRSTARTS(STR(?g), ...))` forces full graph scans — Fuseki can't use its index for string prefix matching on graph URIs. The data is too large for this pattern.

## Recommendation

Replace all 6 client-side SPARQL queries with a single server-side endpoint that reads manifest files.

### New endpoint: `GET /api/harvest/status`

Reads all JSON files from `data/harvest/manifests/` and returns a combined payload:

```json
{
  "domains": {
    "music": { /* contents of music.json */ },
    "photos": { /* contents of photos.json */ },
    "sexuality": { /* contents of sexuality.json */ },
    "stories": { /* contents of stories.json */ },
    "notes": { /* contents of notes.json */ },
    "wordpress": { /* contents of wordpress.json */ }
  },
  "generated": "2026-02-27T20:58:00Z"
}
```

### Dashboard changes (harvest-scope.ejs)

1. Replace `sparql()` calls with single `fetch('/api/harvest/status')`
2. Render counts from manifest data:
   - `stages.load.fuseki_count` → "In Fuseki" number
   - `stages.extract.output_count` → extraction count
   - `sources[].count` → source targets
   - `stages.*.status` → stage progress indicators
   - `tasks[].status` → task completion
   - `gaps[]` → remaining work items
3. Show `stages.*.last_run` timestamps — "where we are tied to manifest runs"
4. Show stage pipeline: extract → transform → load → verify with status per stage
5. Remove the 60s auto-refresh interval (manifest data changes on harvest runs, not continuously)

### Manifest enrichment needed

Some manifests need `fuseki_count` added to their `stages.load` section. The harvester services should update this after sync. For now, a one-time SPARQL query per domain to seed the values is fine — run once, write to manifest, dashboard reads from file thereafter.

Domains needing count enrichment:
- `sexuality.json` — needs model/photo/video/archive counts in stages.load
- `stories.json` — needs story count in stages.load
- `photos.json` — needs `fuseki_count` in stages.load (currently null)

Music already has `fuseki_count: 19609`.

### Why this is better

| Metric | Current (SPARQL) | Manifest-backed |
|--------|-----------------|-----------------|
| Page load | ~120s | <100ms |
| Fuseki load | 6 concurrent aggregation queries | Zero |
| Data richness | Counts only | Stages, tasks, gaps, timestamps |
| Accuracy | Point-in-time query | Last harvest run (source of truth) |

## Implementation

This is a TypeScript route change (needs build + deploy) plus an EJS template rewrite (bind-mounted, no deploy for template). Suggest:

1. Add route handler in `src/app.ts` or a new `src/handlers/harvest-status.handler.ts`
2. Route reads `data/harvest/manifests/*.json`, returns combined JSON
3. Rewrite `harvest-scope.ejs` JavaScript to consume the new endpoint
4. Seed missing fuseki_count values into manifests (one-time script or manual)

## Jeff's Words

> "I want to be able to see the harvest-scopes properly and where we are ties into manifest runs"
