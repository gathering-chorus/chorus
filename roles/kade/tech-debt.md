# Technical Debt Register

Last updated: 2026-03-21

Debt spotted during implementation. Each item includes context, cost of deferral, and suggested resolution.

---

## TD-001: CI Pipeline Non-Blocking — RESOLVED

**Severity:** High
**Found:** 2026-02-13 | **Resolved:** 2026-02-13
**Location:** `.github/workflows/ci-cd.yml`

Removed all `|| echo "skipped"` and `continue-on-error` (except CodeQL which has intermittent infra issues). Removed redundant global TypeScript/ESLint installs. All test/lint/security/build steps now block on failure.

---

## TD-002: 32 Failing Tests — RESOLVED (was stale data)

**Severity:** High
**Found:** 2026-02-13 | **Resolved:** 2026-02-13

The earlier report was based on stale `jest-results.json`. Actual test run: all 1531 tests pass (1498 unit + 3 integration + 20 security + 10 performance).

---

## TD-003: Low Coverage in Auth/ACL Handlers — RESOLVED

**Severity:** Medium
**Found:** 2026-02-13 | **Resolved:** 2026-02-21
**Location:** ACL handler, Collection handler, Group handler

All three handler test suites written from scratch (135 tests total):
- `acl.handler.ts`: 100% statements/branches/functions/lines (53 tests)
- `collection.handler.ts`: 100%/94%/100%/100% (38 tests)
- `group.handler.ts`: 100% all metrics (44 tests)

Previously at ~20% with no test files. `acl.service.ts` was already at 100%.

---

## TD-004: Test Scripts Swallow Failures — RESOLVED

**Severity:** Medium
**Found:** 2026-02-13 | **Resolved:** 2026-02-13
**Location:** `package.json`

Removed `|| true` from all test and lint scripts. Added `--no-coverage` to integration/security/performance scripts (coverage thresholds only meaningful for unit tests). Added `--forceExit` to prevent hanging from open handles.

---

## TD-005: Open Test Handles

**Severity:** Low
**Found:** 2026-02-13
**Location:** Security tests (security-headers.test.ts creates full Express app), integration tests

Tests that import the full Express app don't clean up — server stays alive, keeping the process running. Currently mitigated with `--forceExit`.

**Cost of deferral:** `--forceExit` masks other potential issues. Proper teardown would make tests more reliable.
**Resolution:** Add `afterAll()` hooks in security/integration tests to close the app server. Use `--detectOpenHandles` to identify all sources.

---

## TD-006: ts-jest Deprecated Config

**Severity:** Low
**Found:** 2026-02-13
**Location:** `jest.config.js` — `globals.ts-jest.isolatedModules`

ts-jest warns that `globals.ts-jest` config is deprecated. Should use `transform` config instead. Will break in ts-jest v30.

**Cost of deferral:** Will require migration when ts-jest is upgraded to v30.
**Resolution:** Move ts-jest config from `globals` to `transform` block per ts-jest docs.

---

## TD-007: Broken Owner WebId in 22 ACL Files — RESOLVED

**Severity:** High
**Found:** 2026-02-13 | **Resolved:** 2026-02-19
**Location:** All 20 `jeff/books/*.ttl.acl` + 2 profile card ACLs

ACLs used `acl:agent <https://solidcommunity.net/>` — the OIDC issuer URL, not a valid WebId. Root cause: Pivot auth callback stored issuer URL as session WebId.

**Fix (commit `0fe3d8b`):** Pivot callback now maps known issuer URLs to real WebIds. Removed issuer-as-WebId from authorized-users.ts. Fixed all 22 ACL files on disk via sed. 98 auth tests passing.

---

## TD-008: Missing ACLs for Private Resources — RESOLVED (by design)

**Severity:** Medium
**Found:** 2026-02-13 | **Resolved:** 2026-02-21
**Location:** Property resources

The architectural decision landed on Turtle-driven visibility (`.meta.ttl` with `jb:hasVisibility`), enforced by `collection-visibility.middleware.ts` at the route level. All property READ routes use `propertyVisibility` / `apiPropertyVisibility` middleware. ACL sidecar files are used for per-resource access control (e.g., Wren's read access to books) but are not the primary privacy gate. No ACLs needed for resources where the collection-level visibility is Private.

---

## TD-010: Redundant Inline Auth Checks in Handlers — PARTIALLY RESOLVED

**Severity:** Low → **High** (was blocking public visibility transitions)
**Found:** 2026-02-13 | **Partially resolved:** 2026-02-14
**Location:** `book.handler.ts`, `property.handler.ts` (30+ checks), `gallery.handler.ts`, `book-upload.handler.ts`

Handlers had `if (!req.session.isLoggedIn)` checks duplicating middleware. **This was the root cause of Sprint 2 visibility transition test failures** — when collections were toggled to public, the middleware correctly called `next()` but the handler blocked with 401. Ideas/projects handlers didn't have this problem — they correctly trusted the middleware.

**Resolved:** Removed auth checks from all READ handlers behind visibility middleware (listBooks, getBook, getLocations, renderCollection, getProperty, listHouses, getHouse, listGardens, getGarden, listLands, getLand, servePhoto, servePhotoThumb). Write handlers keep auth checks as defense-in-depth.

**Remaining:** `gallery.handler.ts`, `book-upload.handler.ts` still have inline auth checks. These are admin-only routes so the impact is low.

---

## TD-011: Fuseki @base URI Resolution — RESOLVED

**Severity:** High
**Found:** 2026-02-14 | **Resolved:** 2026-02-14
**Location:** `src/services/sparql.service.ts` → `loadGraph()`

Fragment URIs (`<#book>`, `<#shelf-...>`) resolved against the Graph Store Protocol request URL (`http://localhost:3031/pods/data?graph=...`) instead of the intended graph URI. Invisible for single-graph queries (fragments were consistently wrong but matched within a graph), but broke all cross-graph references. Fixed by prepending `@base <graphUri>` to Turtle content before sending to Fuseki. Full resync applied.

---

## TD-009: E2E Test Artifacts Left on Disk — RESOLVED (already mitigated)

**Severity:** Low
**Found:** 2026-02-13 | **Resolved:** 2026-02-21
**Location:** `e2e/tests/ideas-crud.spec.ts` and other E2E specs

All E2E tests already include cleanup steps that delete created resources on success. Artifacts only remain when tests fail mid-run (cleanup step never reached). No orphaned artifacts currently on disk. This is inherent to E2E tests hitting real data — would require a dedicated test pod to fully eliminate, which is out of scope for current priorities.

---

## TD-012: Ghost/Nginx Dead Code in app-state.sh — RESOLVED

**Severity:** Medium
**Found:** 2026-02-16 | **Resolved:** 2026-02-16
**Location:** `app-state.sh`

Removed all Ghost and Nginx references: header comment, port checks (8446, 2368), ghost_content directory creation, Stage 3/4 deploy blocks (Ghost + Nginx), Ghost URL output, ghost/nginx from OTHER_CONTAINERS array, Ghost/Nginx status checks, Ghost/Nginx log sections, and help text. ~50 lines of dead code removed.

---

## TD-013: Stale PID File After app-state.sh restart — RESOLVED

**Severity:** Low
**Found:** 2026-02-16 | **Resolved:** 2026-02-21
**Location:** `app-state.sh` PID tracking vs Docker-managed Express app

app-state.sh was rewritten (2026-02-18) to use Docker container names exclusively — no PID files. `container_running()` and `container_exists()` check Docker state directly. The PID file concern is moot.

---

## TD-014: ffmpeg Dependency Not in Dockerfile — RESOLVED

**Severity:** Medium
**Found:** 2026-02-16 | **Resolved:** 2026-02-21
**Location:** `Dockerfile` line 40

ffmpeg was added to the runtime stage of the Dockerfile (`apk add --no-cache build-base python3 ffmpeg`) as part of the voice/video capture work (2026-02-19, card #75). Video transcoding works in Docker.

---

## TD-015: Whisper Built From Source on Every Docker Cache Bust

**Severity:** Medium
**Found:** 2026-02-20
**Location:** `Dockerfile` — whisper.cpp build stage

Every Docker cache bust triggers a full whisper.cpp C++ compilation + 150MB model download. This adds ~2 min to deploys and introduces non-deterministic failure modes (network issues, compiler version drift).

**Cost of deferral:** Slower deploys when Dockerfile/package.json changes. Flaky builds when HuggingFace CDN is slow.
**Resolution:** Pre-build whisper.cpp + model as a tagged base image (`jeff-bridwell-personal-site-whisper:latest`). Reference in multi-stage Dockerfile. One-time build, then stable.

---

## TD-016: E2E Tests Overwrite Runtime Pod Data

**Severity:** Low
**Found:** 2026-02-20
**Location:** `data/pods/jeff/` — ACL files, users.ttl

Playwright E2E tests (run by pre-commit hook) write to the real pod data directory via the bind-mounted app. This resets ACL entries and user data added at runtime. Currently requires manual re-setup after commits.

**Cost of deferral:** Agent ACL entries (e.g., Wren's Read access to books) get lost after every commit that runs E2E tests.
**Resolution:** Use a separate pod data directory for E2E tests, or snapshot/restore data around test runs.

---

## TD-017: ICD Consumer Type Names Mismatch Actual RDF Types

**Severity:** Low (mitigated)
**Found:** 2026-03-19
**Location:** `icd.service.ts` compliance endpoint, ICD graph

ICD consumer types used aspirational names (CanonicalPerson, CanonicalPhoto, CanonicalTrack) that didn't match actual graph types (Person, Photo, Track). Fixed by: (1) updating graph type names to match reality, (2) adding Canonical→base fallback in compliance query as safety net. Root cause: migration script hardcoded "Canonical" prefix without checking ontology.

---

## TD-018: ICD Write Endpoint Logic Duplicated Across App and Chorus API

**Severity:** Medium
**Found:** 2026-03-19
**Location:** `src/services/icd.service.ts` + `chorus/api/src/server.ts`

ICD write endpoints (POST fields, POST mappings, PUT sections) exist in both the app (:3000, session auth) and Chorus API (:3340, no auth). Same SPARQL UPDATE patterns duplicated. If triple structure changes, both must be updated.

**Cost of deferral:** Divergent behavior between surfaces if one is updated without the other.
**Resolution:** Extract shared ICD write logic into a module both servers import, or proxy from Chorus API to app's internal API with service token.

---

## TD-019: Old Story TTLs Had Broken Triple-Quoted Strings

**Severity:** Low (resolved)
**Found:** 2026-03-20 | **Resolved:** 2026-03-20
**Location:** `data/pods/jeff/stories/*.ttl`

Old story harvester wrote body strings with actual newlines inside single-quoted Turtle literals, causing TriG assembly to fail with "Broken token (newline in string)." Fixed by deleting old TTLs and re-harvesting from stories.md sources. 45 files had broken triple-quote escaping.

---

## TD-020: StoryHarvesterService Requires Full DI Chain for Standalone Use

**Severity:** Low
**Found:** 2026-03-20
**Location:** `src/services/story-harvester.service.ts`

Running the story harvester outside the app context requires manually wiring PodWriteService (with FusekiSyncService stub), StoriesPodService, and FusekiSyncService. The `createEpisodeWithEdges` method calls `this.fusekiSync.syncResourceFireAndForget()` with no null guard — passing null crashes. Harvest scripts need stubs.

**Cost of deferral:** Every standalone harvest script needs boilerplate DI setup.
**Resolution:** Add null guard on fusekiSync calls, or provide a factory method for standalone harvest use.

---

## TD-021: Sexuality Graph Structure Blocks Search Indexing

**Severity:** High
**Found:** 2026-03-21
**Location:** `urn:jb:sexuality/volumes/*.ttl`, `search-index.service.ts`

2M+ items (1.7M images, 94K videos, 40K archives, 22K models) all in `volumes/*.ttl` graphs mixed by type. Search can't efficiently index by type without scanning every volume. `GRAPH.media` constant points to `urn:jb:media/` which has zero graphs — entities use that URI prefix but live in sexuality volume graphs.

**Cost of deferral:** Sexuality domain unsearchable (stuck at 20K stale model entries). 2M items invisible to FTS.
**Resolution:** Card #1584 — restructure to type-specific graphs (video/, image/, archive/, model/).

---

## TD-022: ICD TTL Files Overwritten by migrate-icd-tree.py

**Severity:** Medium
**Found:** 2026-03-21
**Location:** `scripts/migrate-icd-tree.py`, `src/ontology/icd-instance-*.ttl`

Running `migrate-icd-tree.py` (with or without `--publish`) regenerates all TTL files from SEMANTIC_MAPPER.html, overwriting manually-added properties (implementation contract, canonicalNamespace, valueStreamStage). SEMANTIC_MAPPER.html is now deleted (#1569) so the script is broken for regeneration. TTL files are the source of truth but the script doesn't know that.

**Cost of deferral:** Accidental `migrate-icd-tree.py` run wipes ICD extensions.
**Resolution:** Remove the HTML-parsing regeneration path from migrate-icd-tree.py. Keep only the --publish (GSP PUT) path that reads existing TTL files.

---

## TD-023: Canonical Photo Graph Built From Wrong Source Priority

**Severity:** High
**Found:** 2026-03-23
**Location:** `urn:jb:photos/canonical/` (Fuseki)

63K canonical photo subjects, 77% sourced from Takeout (shallowest source), 23% from Apple, 0% from iPhone. Field coverage: 18% dimensions, 12% GPS, 6% people — when Apple and iPhone sources have 100% dimensions and 89% GPS. 60K records are single-source with no cross-matching. ~18K may be Takeout multi-album duplicates.

**Root cause:** Canonical was built volume-first from Takeout before richness analysis was done. Two of three sources (Apple SQLite full extract, iPhone) were never loaded as source graphs.

**Cost of deferral:** Every photo page, search result, and analytics query inherits Takeout's metadata poverty. Jeff doesn't trust the data.
**Resolution:** Rebuild canonical from scratch using Jeff-confirmed 4-era authority model (Apple primary through 2019, iPhone 2020+). Depends on #1642 (era-scoped ICD).

<!-- #3013 integration probe 2026-05-19: exercising post-#3012 /acp chain end-to-end. This line is the throwaway edit. -->
