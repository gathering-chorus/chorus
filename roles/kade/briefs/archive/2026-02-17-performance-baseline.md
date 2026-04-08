# Brief: Non-Functional Performance Baseline

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-17
**Priority**: P1 (baseline must be captured BEFORE storage migration starts)
**References**: ADR-007 (storage topology)

---

## Context

We're about to migrate ~1.9TB of source media from the primary Mac to the Mac mini via SMB (ADR-007). Before we change anything, we need a performance baseline of the running system. After each migration phase, we re-run the same tests and compare. This gives us empirical proof that the storage change didn't degrade anything — or tells us exactly what degraded and by how much.

Jeff's directive: baseline on local first, rebaseline after the change.

## What Exists Today

- `tests/performance/load-test.test.ts` — stub app, not useful for real benchmarks
- `tests/performance/gallery-performance.test.ts` — good for in-process algorithm benchmarks, but mocked I/O
- `scripts/startup-profiler.sh` — wall-clock infrastructure timing
- Prometheus `http_request_duration_seconds` — production histogram (default buckets)

None of these give us a **repeatable, comparable baseline** against real endpoints with real data.

## What to Build

A benchmark script at `scripts/benchmark.sh` (or `scripts/benchmark.ts`) that:

1. Hits real endpoints on the running app (localhost:3000)
2. Measures response times (min, avg, P50, P95, P99, max)
3. Records results to `benchmarks/<timestamp>.json`
4. Can be run manually: `npm run benchmark` or `./scripts/benchmark.sh`
5. Has a `--compare <file>` mode that diffs two benchmark files and flags regressions

### Endpoints to Benchmark

Each endpoint gets **50 requests** (enough for stable percentiles, not enough to stress the system — this is a single-user app).

#### Tier 1: Core Pages (full stack — Express + middleware + pods)

| Endpoint | Method | Notes |
|----------|--------|-------|
| `GET /` | GET | Home page |
| `GET /profile` | GET | Profile + mind map |
| `GET /collection/books` | GET | Book listing (pod read) |
| `GET /collection/ideas` | GET | Ideas listing (pod read) |
| `GET /collection/property` | GET | Property listing (pod read) |
| `GET /collection/blog` | GET | Blog listing (pod read) |

#### Tier 2: Music (heaviest workload — 5,800+ albums in Fuseki)

| Endpoint | Method | Notes |
|----------|--------|-------|
| `GET /collection/music` | GET | Album grid (paginated, Fuseki query) |
| `GET /collection/music?page=2` | GET | Pagination performance |
| `GET /collection/music?search=beatles` | GET | Search (Fuseki text query) |
| `GET /collection/music?genre=Rock` | GET | Genre filter (Fuseki query) |
| `GET /collection/music/<album-slug>` | GET | Album detail (cross-graph join) |

For the album detail test, pick 3 albums with known slugs and average the results.

#### Tier 3: API Endpoints

| Endpoint | Method | Notes |
|----------|--------|-------|
| `GET /api/health` | GET | Health check baseline |
| `GET /api-docs` | GET | Swagger docs |
| `GET /metrics` | GET | Prometheus metrics endpoint |

#### Tier 4: Fuseki Direct (SPARQL)

| Query | Notes |
|-------|-------|
| Simple: `SELECT (COUNT(*) as ?c) WHERE { ?s ?p ?o }` | Total triple count |
| Cross-graph: Album→Artist join | The pattern from ADR (pending) |
| Aggregation: Albums by genre with counts | Browse view backing query |
| Search: Text match on album/artist name | Search performance |

Hit Fuseki directly at `localhost:3030/gathering/sparql` with these queries, 20 requests each.

### Output Format

```json
{
  "timestamp": "2026-02-17T08:30:00Z",
  "label": "pre-migration-baseline",
  "system": {
    "hostname": "Jeffs-Mac-Mini-M1-3",
    "disk_used_pct": 99,
    "disk_free_gb": 13,
    "fuseki_triple_count": 54331
  },
  "results": {
    "/": {
      "requests": 50,
      "min_ms": 12,
      "avg_ms": 18,
      "p50_ms": 16,
      "p95_ms": 28,
      "p99_ms": 45,
      "max_ms": 52,
      "errors": 0
    },
    ...
  }
}
```

### Compare Mode

`./scripts/benchmark.sh --compare benchmarks/pre-migration.json benchmarks/post-phase1.json`

Output a table showing:
- Endpoint, before P95, after P95, delta, regression flag (>20% slower = yellow, >50% = red)

### Implementation Approach

**Option A: Bash + curl** — simplest, no dependencies, works everywhere
- `curl -w "%{time_total}" -o /dev/null -s` for each endpoint
- `jq` for JSON output
- ~100 lines of bash

**Option B: Node.js script** — cleaner stats, better SPARQL support
- `node-fetch` or built-in `fetch` for HTTP
- Direct Fuseki SPARQL queries via HTTP POST
- percentile calculations in JS
- ~200 lines of TypeScript

**Recommendation**: Option A for HTTP endpoints (simpler, no build step), with a small Node helper for SPARQL queries if needed. Or all Node if you prefer consistency.

## When to Run

| Label | When | Notes |
|-------|------|-------|
| `pre-migration-baseline` | **NOW** (before any storage changes) | Capture this first |
| `post-phase1-smb-mount` | After SMB mount + off-machine backups working | Should show no change (nothing moved yet) |
| `post-phase2-media-migrated` | After ~1.9TB media moved to SMB | SSD freed up — might improve I/O |
| `post-docker-prune` | After Docker cleanup | Reclaimed space might improve container perf |

## Acceptance Criteria

- [ ] Benchmark script runs against real localhost:3000 + localhost:3030
- [ ] Measures all Tier 1-4 endpoints
- [ ] Outputs JSON to `benchmarks/<label>.json`
- [ ] Compare mode produces a readable delta table
- [ ] **Pre-migration baseline captured and committed to repo**
- [ ] Script added to package.json as `npm run benchmark`

## Constraints

- **Don't install heavy load testing tools** (k6, artillery). This is a single-user system — simple timing is sufficient.
- **Don't modify the app** to add benchmark support. Hit it from the outside.
- **50 requests per endpoint max.** We're measuring latency, not throughput.
- **Fuseki must be running** for Tier 2 and Tier 4 tests. Script should gracefully skip if Fuseki is down.
- **Results go in `benchmarks/` directory** at repo root, committed to git for historical comparison.

## Sequencing Note

**This must run BEFORE the storage migration brief (2026-02-17-storage-migration.md).** The baseline is only valuable if captured before any changes. Recommended order:

1. Build benchmark script
2. Run pre-migration baseline
3. Then start Phase 1 of storage migration

---

— Silas
