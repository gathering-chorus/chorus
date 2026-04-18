# Test Triage — 2026-04-18

**Card:** [#2214](https://) — Integration triage spike
**Contract:** [TEST.md](./TEST.md)
**Scope:** Every integration-mode failure + every skip, classified against the TEST.md binary rule.

## Scope

- **43 quarantined suites** in `platform/api/jest.config.js` `testPathIgnorePatterns` (214 tests total).
- **5 mixed files** with env-var-conditional describes in `platform/api/tests/` (132 skips under default).
- **1 failing file** in `directing/clearing/tests/tiles.test.ts` (2 failures).
- Total: **346 skipped + 22-48 failing** (failure count varies per run due to service-state flakiness).

## Buckets (from TEST.md)

- **fix** — assertion passes, test stays in its current mode, 0/0 reached.
- **delete** — assertion is dead (feature removed, moved, or the test was never real). Commit message states what it was asserting and why.
- **migrate** — test is legitimate but runs against real services when it could run hermetic via #2208 pattern (oxigraph in-process + fixture TTL). Moves from integration-gated to hermetic-always.
- **legitimately-integration** — network/SSH/cross-machine probes, Fuseki-specific TDB2 behavior, or other concerns that a hermetic shim can't cover honestly. Stays gated, small suite.

Each row flags **slow-but-correct** (✓ = runs in single digit seconds when its services are up; ✗ = slow enough that default-mode inclusion would blow the fast-feedback budget).

## Table — Quarantined suites (43 files, 214 tests)

| Suite | Tests | Classification | Reason | Slow? | Shape |
|-------|------:|----------------|--------|:-----:|-------|
| server-unit | 4 | **delete** | Kade's own example cited in #2196 — asserts `status=0` (timeout) as pass, zero response-shape assertions. Extracted handlers cover the real logic. | ✓ | S |
| rca | 8 | **fix** | 2 persistent failures (POST /api/chorus/rca creates, links). Real contract, real failure. Service-state dependent. | ✗ | M |
| completeness-perf | 5 | **migrate** | Perf budget against real Fuseki — inherently slow. Migrate to hermetic oxigraph harness where response time is deterministic; assert shape, not timing, or run perf in a separate suite. | ✗ | M |
| graph-separation | 1 | **delete** | Boots full app + reloads ontology. Integration-masquerading. Handler unit tests on athena-validate cover the real integrity surface. | ✗ | S |
| observability | 7 | **migrate** | Asserts observability endpoint shape against live chorus-api. Hermetic via mocked spine store. | ✓ | M |
| logs-facet | 7 | **migrate** | Logs endpoint shape — hermetic via fixture TTL + mocked log paths. | ✓ | M |
| deploys | 3 | **legitimately-integration** | Asserts real launchctl state. Hermetic shim would lie. Keep gated, small. | ✗ | — |
| shacl-validation | 4 | **migrate** | SHACL constraint checks against real ontology. `#2208` pattern directly applies — oxigraph + fixture TTL. | ✓ | M |
| crawl-shape | 6 | **migrate** | Page-shape assertions; hermetic via fixture HTML or mocked crawler. | ✓ | M |
| hooks-summary | 8 | **migrate** | hooks.log shape; hermetic via fixture log file. | ✓ | S |
| jeff-summary | 7 | **migrate** | Jeff-posture summary shape; hermetic via fixture cache. | ✓ | S |
| quality-summary | 9 | **migrate** | Quality metrics shape; hermetic via fixture quality.json. | ✓ | M |
| trace-convergence-callstack | 3 | **migrate** | Trace stitching; hermetic via fixture trace JSON. | ✓ | M |
| alerts-subdomain | 2 | **migrate** | Alerts facet shape; hermetic via fixture yaml dir. | ✓ | S |
| assessment | 5 | **migrate** | Assessment shape; hermetic via fixture snapshot. | ✓ | M |
| cost-summary | 9 | **migrate** | Cost ledger shape; hermetic via fixture cost.json. | ✓ | M |
| discover-endpoints | 5 | **migrate** | Endpoint discovery against live server — hermetic via AST-based fixture source. | ✓ | M |
| discover-pages | 5 | **migrate** | Page discovery — hermetic via fixture views/ tree. | ✓ | M |
| domain-dependencies | 5 | **migrate** | Dependency graph shape; hermetic via oxigraph + fixture TTL. | ✓ | M |
| fitness-summary | 8 | **fix then migrate** | 1 persistent failure (4 fitness metrics). Fix the broken assertion first, then migrate the suite hermetic. | ✗ | M |
| ollama-resilience | 4 | **legitimately-integration** | Asserts Ollama network fallback behavior. Hermetic would fake the very thing being tested. | ✗ | — |
| patterns-summary | 7 | **migrate** | Patterns summary shape; hermetic via fixture spine DB. | ✓ | M |
| scheduled-reindex | 5 | **delete-or-migrate** | Tests the scheduled reindexer. Examine: is the scheduler worth testing at this layer, or is per-source reindex logic covered elsewhere? | ✓ | S |
| search-freshness | 5 | **migrate** | Freshness-tier logic; hermetic via in-process LanceDB + fixture index. | ✓ | M |
| session-replay | 6 | **migrate** | Session replay endpoint shape; hermetic via fixture session DB. | ✓ | M |
| trace-envelope | 5 | **migrate** | Envelope assembly; hermetic via fixture deps (already close). | ✓ | S |
| borg-landing | 3 | **migrate** | Landing-page shape; hermetic via fixture HTML. | ✓ | S |
| chorus-landing | 4 | **migrate** | Same pattern as borg-landing. | ✓ | S |
| code-inventory | 2 | **migrate** | Code-file inventory shape; hermetic via fixture graph. | ✓ | S |
| crawl-validation | 3 | **migrate** | Crawl output shape; hermetic via fixture crawl result. | ✓ | S |
| domain-api-consolidated | 10 | **migrate** | Broadest domain API shape — direct fit for #2208's pattern (Wren already exemplifies in #2208). Merge into her regression suite? | ✓ | L |
| domain-borg-services | 5 | **migrate** | Borg services shape; hermetic via oxigraph + fixture instances. | ✓ | M |
| domain-pipeline | 7 | **migrate** | Pipeline shape; hermetic via oxigraph + fixture. | ✓ | M |
| domain-radius | 6 | **migrate** | Blast-radius shape; hermetic via oxigraph. | ✓ | M |
| domain-releases | 5 | **migrate** | Release metadata shape; hermetic via fixture. | ✓ | M |
| domain-section-enrichment | 3 | **fix** | 1 persistent failure (Pulse itemDetail carries description+reads+writes). Real regression after #2206 enrichment work. Fix the actual assertion; don't hide. | ✓ | M |
| in-process-harness | 3 | **delete** | Tests the old startTestApp harness being removed. Dead weight. | ✓ | S |
| instance-explorer | 4 | **migrate** | Instance-explorer UI route shape; hermetic via fixture DOM. | ✓ | M |
| spine-event-endpoint | 4 | **migrate** | Spine event write/read shape; hermetic via fixture spine store. | ✓ | M |
| tests-domain-code | 3 | **migrate** | Tests/domain/code cross-reference; hermetic via fixture graph. | ✓ | S |
| timestamp | 2 | **migrate** | Timestamp normalization; hermetic already feasible (likely just needs fakes for time source). | ✓ | S |
| trace-batch-callstack | 3 | **migrate** | Trace batch aggregation; hermetic via fixture trace JSON. | ✓ | S |
| trace-integration-callstack | 4 | **migrate** | Trace stitching across services; hermetic via fixture. Name is misleading — the test itself is integration-masquerading, the content can be hermetic. | ✓ | M |

**Quarantined-suite totals:** 214 tests across 43 files. Buckets: fix=3 files / delete=4 files / migrate=34 files / legitimately-integration=2 files / fix-then-migrate=1 file / delete-or-migrate=1 file.

## Table — Mixed files (5 files with env-var-conditional describes, 132 skips)

These violate the binary rule. Per TEST.md, each file splits into two: hermetic-always describes stay in the original file, integration-gated describes move to a new `<name>-integration.test.ts` file (or get migrated hermetic).

| File | Skips | Classification | Plan |
|------|------:|----------------|------|
| athena.test.ts | ~35 | **split + migrate** | Large mixed file (909 lines). Hermetic blocks stay as `athena.test.ts`; integration blocks move to `athena-integration.test.ts`. Migrate the integration ones to oxigraph per #2208 — this test file is the seed for what #2208 already validates. Substantial but high-value. | 
| embed-sync.test.ts | ~20 | **split + migrate** | Hermetic: embedding math, vector shape. Integration: real Ollama calls — move to integration suite OR migrate to fixture embedding. |
| perf-budget.test.ts | ~30 | **delete-or-migrate** | 7 persistent failures (all perf budgets blown in this run). Real-service perf assertions against live Fuseki are inherently flaky. Recommend: delete the perf assertions at this layer; move latency monitoring to observability (ops), not to the test suite. |
| search-ax-quality.test.ts | ~25 | **split + migrate** | Search quality assertions. Integration against live LanceDB; hermetic via fixture vector store. |
| trace-envelope.test.ts | ~22 | **split + migrate** | Also in the 43 quarantined list. Deduplicate: pick either the mixed-file split or the quarantine migration, not both. |

**Mixed-file totals:** ~132 skipped describes across 5 files. All violate binary rule. All need file-level split before or during migration.

## Table — Clearing failures (1 file, 2 failures)

| File | Failures | Classification | Reason |
|------|:--------:|----------------|--------|
| directing/clearing/tests/tiles.test.ts | 2 | **fix** | Cheap immediate win per TEST.md conversation. Needs to be read to determine specific failure — likely stale assertion after a clearing UI refactor. |

## Summary

| Bucket | File count | Test count (approx) |
|--------|-----------:|---------------------:|
| fix | 4 | ~14 |
| delete | 5 | ~14 |
| migrate | 37 | ~275 |
| legitimately-integration | 2 | 7 |
| fix-then-migrate | 1 | 8 |
| split + migrate | 4 | ~102 (mixed-file halves) |
| split or dedup (trace-envelope) | 1 | ~5 |
| delete-or-migrate | 2 | 35 |

## Cards to file off this table (NOT filed from this spike per discipline)

The spike output informs the next wave; filing is Jeff's call.

- **Delete sweep** — bucket=delete rows (5 files, ~14 tests). Single focused session. Each deletion commit names what was asserted + why coverage moved. Small.
- **Fix sweep** — bucket=fix rows (4 files, ~14 tests). 2 clearing + rca (2) + domain-section-enrichment (1) + fitness-summary (1). Inline as sessions touch, OR batched as one focused day. Small-to-medium.
- **Migration cards per cluster** — migrate rows (37 files). Group by domain:
  - `athena-*` cluster (big; pair session, #2208 pattern)
  - `domain-*` cluster (blast-radius, pipeline, releases, borg-services, dependencies — oxigraph pattern, maybe folded into #2208 extension)
  - `*-summary` cluster (hooks-summary, jeff-summary, quality-summary, cost-summary, patterns-summary, fitness-summary — all fixture-JSON pattern)
  - `discover-*` cluster (endpoints, pages)
  - Standalone migrations (observability, search-freshness, session-replay, spine-event-endpoint, etc.)
- **Mixed-file split sweep** — 5 mixed files split into hermetic + integration before migrations. Could be prerequisite for each migration card in that cluster.
- **Baseline update** — after first cleanup pass lands, refresh TEST.md honesty metric table (platform/api 48 → N, 346 → N).

## Post-triage honesty metric

When migration cards land:

| Milestone | Default passes | Default skips | Integration passes | Integration failures |
|-----------|---------------:|--------------:|-------------------:|---------------------:|
| Today (2026-04-18) | 778 | 132 | 1094 | 22-48 |
| After delete sweep | ~778 | ~132 | ~1080 | ~15-40 |
| After fix sweep | ~778 | ~132 | ~1100 | 0-2 |
| After mixed-file split | ~775 | 0 | ~1100 | 0-2 |
| After summary-cluster migration | ~825 | 0 | ~1050 | 0-2 |
| After domain-cluster migration | ~875 | 0 | ~1000 | 0-2 |
| After full migrate | ~1100 | 0 | ~20 | 0 |
| Final (legitimately-integration only) | ~1100 | 0 | 7 | 0 |

Target reached when both columns are 0 errors + default jest hits every test that can be hermetic.

## Classification confidence

- **High**: files I read or have context for from today's work — server-unit, rca, domain-section-enrichment, perf-budget, fitness-summary, athena, graph-separation, trace-envelope, tiles.
- **Medium**: classified from filename pattern + integration-smell content (grep-based). The `migrate` bucket is mostly this — they LOOK like contract tests against live services with no structural reason they couldn't be hermetic.
- **Low**: `scheduled-reindex` and `trace-integration-callstack` need visual inspection to confirm the recommendation.

Where confidence is medium, first migration attempt will surface the real answer — if a file turns out to have a legitimately-integration reason (network, cross-machine, timing-sensitive), reclassify at migration time.
