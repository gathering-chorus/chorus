# Brief: Search Performance — 42s Query Time

**From:** Wren | **To:** Kade | **Date:** 2026-03-03 | **Card:** #850

## Finding

Added `durationMs` logging to search.handler.ts (committed `afdf698`, deployed). First measured result:

```
"deleuze" | fts-page | 42,409ms | 6 results
```

42 seconds for 6 results. The 5min stats cache helped page render but the FTS5 MATCH query itself is the bottleneck — not `getStats()`.

## Root Cause

1.8M sexuality items in the same FTS5 table as 14K general items. Every MATCH scans the full index. BM25 ranking doesn't help if the scan takes 42s before ranking starts.

## Recommended Fix

**Both UX and engineering.** Jeff: "We are doing both — I believe we can make this performant on large data sets."

**1. Partition the FTS index.** Two tables:

- `search_fts` — general content (14K items: notes, music, books, stories, blogs). Should query in <100ms.
- `search_fts_sexuality` — sexuality content (1.8M items). Queried when sexuality checkbox is on.

The `search_items` base table stays unified. Only the FTS virtual tables split. Triggers route inserts by collection. Query logic checks which collections are selected and queries the right FTS table(s).

**2. Make sexuality search fast too.** 1.8M is not inherently slow for FTS5 — SQLite handles it. The issue may be the contentless FTS joining back to `search_items` for every result. Options:
- Content-bearing FTS (duplicates data but avoids join)
- Limit FTS MATCH results before joining (`LIMIT` inside the FTS query, not after)
- Index tuning — ensure the FTS tokenizer and prefix indexes are optimal for the data shape

Target: <2s for large collection searches, <100ms for general.

**3. Design for scale.** Sexuality (1.8M) is the current stress test, but photos will be in the same ballpark once Google Photos is harvested. The FTS solution must handle any collection at millions of rows — not just partition around one collection.

## UX Change — Jeff's Direction

Replace the collection dropdown with **checkboxes**. Default: all collections checked ON except sexuality OFF. User opts *in* to the heavy collection, not out of the fast ones.

This also solves multi-collection search — dropdown forces "pick one," checkboxes let you compose (e.g., notes + stories + music).

## AC Update

- [ ] Collection filter is checkboxes, not dropdown
- [ ] Default: all collections on, sexuality off
- [ ] General search (sexuality unchecked) returns in <500ms
- [ ] Sexuality search works when checked on
- [ ] `durationMs` logged for every query (already done)
- [ ] Second search after cold start still <500ms (cache warm)

## Perf Test Harness — Jeff's Direction

Build a repeatable benchmark script. Run both FTS and semantic paths. Iterate against target numbers, not vibes.

**Queries (run each 3x, both modes):**

| Query | Type | Why |
|-------|------|-----|
| deleuze | philosophical | multi-collection, small result set |
| heidegger | philosophical | same shape, different term |
| wren | common word | tests broader match volume |
| hey jude | song lookup | music collection coverage |
| desiring production | concept | semantic strength test |
| garden soil | practical | cross-domain (notes, stories) |

**Modes per query:**
- FTS: `/api/search?q=...`
- Semantic: `/api/search?q=...&semantic=true`

**Report:** p50 / p95 / max per query per mode. Output as JSON for Loki/Grafana ingestion.

**Targets:**

| Metric | FTS | Semantic |
|--------|-----|----------|
| General search p95 | <500ms | <2s |
| Sexuality included p95 | <2s | <3s |
| Cold start first query | <5s | <5s |
| Index rebuild | <60s | n/a |

Script location: `scripts/search-benchmark.sh` — Kade runs after every change, sees the delta.

## Files

- `src/services/search-index.service.ts` — FTS table creation, triggers, search method
- `src/handlers/search.handler.ts` — timing already added (this session)
- `views/search.ejs` — collection filter UI (dropdown → checkboxes)
- `scripts/search-benchmark.sh` — new, perf test harness
