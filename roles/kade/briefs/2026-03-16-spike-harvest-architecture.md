# Spike: Harvest Architecture Contract

**Card**: #1456 | **Author**: Silas | **Date**: 2026-03-16
**Trigger**: Photos session 2026-03-15 — wild count fluctuations (283K → 382K → 135K → 60K), slug mismatches, dual-property confusion, ad hoc scripts in /tmp

---

## The Problem

Every harvest is a snowflake. When Kade generates thumbnails or loads photos into Fuseki, there's no contract governing what a "complete harvest" looks like. The result: four different systems report four different counts for the same domain, and nobody knows which one to trust.

Yesterday's Photos session surfaced three failure modes that recur across domains:

1. **Graph inflation** — 382K Photo triples vs 135K actual files. Multiple import passes created duplicate records across different graph partitions (unalbummed monthly graphs + albummed individual graphs + import-photosnew).

2. **Property confusion** — `photoSource` vs `harvestSource` tag the same photos differently depending on which code path created them. The app has two separate functions (`getDrivePhotos` + `getGoogleTakeoutPhotos`) that don't know about each other.

3. **No single count** — filesystem says 135K, Fuseki says 382K, SQLite says 24K, app says 283K. Each is correct for what it measures, but there's no authoritative "how many photos do we actually have?"

## Current State by Domain

| Domain | Manifest | ETL Stages | Target Count | Reconciliation | Maturity |
|--------|----------|------------|-------------|----------------|----------|
| Music | yes | extract→transform→load | yes (129K tracks) | yes (Fuseki count tracked) | **Curated** |
| People | yes | yes | yes | yes | **Curated** |
| Documents | yes | yes | no | no | Somewhat |
| Photos | yes | no | no | no | **Ad hoc** |
| Notes | yes | no | no | no | Ad hoc |
| Stories | yes | no | no | no | Ad hoc |
| Blog | yes | no | no | no | Ad hoc |
| Facebook | yes | no | no | no | Ad hoc |
| LinkedIn | yes | no | no | no | Ad hoc |
| Sexuality | yes | no | no | no | Ad hoc |

Music is the only domain that tracks its target count in the manifest and validates Fuseki against it. Photos — the domain with the most data — has no stages, no target, no reconciliation.

## What "Curated" Means

A curated harvest has five properties:

### 1. Source Registry
Each domain declares its sources in the manifest with a stable ID. A source is a place data comes from (Google Takeout, Apple Photos Library, Bedroom filesystem). New sources get registered, not invented ad hoc.

**Music has this.** 7 sources, each with an ID. Photos has 2 sources in the manifest but at least 4 actual import paths (Google Takeout TTLs, Drive API, Apple SQLite, import-photosnew).

### 2. ETL Stages
Extract → Transform → Load, tracked per source. Each stage records what it produced and when. A failed stage doesn't proceed to the next.

**Music has this.** Each source tracks extract/transform/load independently. Photos has no stage tracking — a script runs, produces files, and hopes for the best.

### 3. Authoritative Count
One number per domain that everyone trusts. Defined as: "how many {things} do we have that a user can see?" Derived from the target system (Fuseki), not from any intermediate artifact.

**Music has this** (129K tracks in manifest target). Photos doesn't — yesterday we couldn't agree whether it was 60K, 135K, or 382K.

### 4. Reconciliation
After load, verify: count in Fuseki matches count from transform output. Flag drift. The music harvester does this; it warns when "new count suspiciously low."

### 5. Idempotency
Running the same harvest twice produces the same result. No duplicates, no phantom records. This is where Photos failed hardest — multiple import passes inflated the graph 3x.

## Source of Truth Map

This is what was missing yesterday. For each domain, which system is authoritative:

| Question | Photos (current) | Photos (should be) |
|----------|-----------------|-------------------|
| How many photos exist? | Nobody knows | Manifest target count, derived from Fuseki WHERE thumbnailPath EXISTS |
| What's the filename? | TTL `photoFilename` or SQLite | Fuseki (canonical after harvest) |
| Where's the source file? | Bedroom filesystem | Bedroom filesystem (Fuseki stores path) |
| Where's the thumbnail? | Bedroom filesystem | Bedroom filesystem (Fuseki stores path) |
| Is it visible to Jeff? | App cache (filtered by thumbnailPath) | Same, but count should match manifest |

## The Three Patterns That Broke Photos

### Pattern 1: Multiple import paths, no dedup gate
Google Drive harvester and Google Photos harvester both imported photos. Drive created 285K records with `photoSource=google-drive`. Photos created 68K records with `harvestSource=google-takeout`. Some overlap, different properties, different graph structures. No gate to prevent this.

**Fix**: One canonical import path per source. If Google Takeout is the source, Drive records for the same photos shouldn't exist. Dedup at harvest time, not at display time.

### Pattern 2: Graph-per-record vs graph-per-partition
Google Takeout photos: 68K individual named graphs (one per photo). Unalbummed photos: 204 monthly partition graphs. Import-photosnew: one bulk graph. Three different graph strategies for the same domain.

**Fix**: One graph strategy per domain. For photos, monthly partitions make sense (matches the date-based browse pattern). Individual graphs per photo are expensive and unnecessary.

### Pattern 3: No filesystem ↔ graph reconciliation
382K graph records, 135K actual files. Nobody noticed the 3x inflation because there's no check that says "graph count should ≈ file count."

**Fix**: Post-load reconciliation step. Count files on Bedroom, count triples in Fuseki, flag if delta > 5%.

## Recommended Harvest Contract

Every domain harvest must:

```
1. DECLARE sources in manifest (with stable IDs)
2. TRACK stages (extract → transform → load) per source
3. PRODUCE a target count after load (how many visible items?)
4. RECONCILE against filesystem (count files) and Fuseki (count triples)
5. BE IDEMPOTENT — re-running produces same result, not duplicates
6. USE ONE graph strategy per domain (partition scheme declared in manifest)
7. USE ONE property vocabulary per domain (no photoSource vs harvestSource splits)
```

## Next Steps

1. **Photos first** — it's the most broken and the most visible to Jeff. Apply the contract to photos: unify the import path, reconcile counts, set the target.
2. **Music as template** — music already follows most of the contract. Extract the pattern into a shared harvester base or documented standard.
3. **Card the remaining domains** — notes, stories, blog, etc. can adopt incrementally. Don't boil the ocean.

## For Wren
This is an architecture spike, not a feature. The output is the contract above + the source-of-truth map. Implementation cards come from this — they should be sized and prioritized by Wren based on which domains Jeff cares about most right now.

## For Kade
The immediate action from yesterday: photos needs the contract applied. Unify `photoSource` and `harvestSource` into one property. Set the manifest target. Add post-load reconciliation. The 17K thumbnail gen running overnight is fine — but the next harvest should follow the contract, not another /tmp script.
