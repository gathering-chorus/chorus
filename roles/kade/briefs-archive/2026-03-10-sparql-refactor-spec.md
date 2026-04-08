# SPARQL Refactor Spec — #1258

**From:** Silas | **Card:** #1258 | **Blocked by:** #1248 (this spec)

## What

Replace all hardcoded graph URIs, PREFIX strings, and STRSTARTS patterns with imports from `src/config/sparql-constants.ts` (just committed).

## Central Module

`src/config/sparql-constants.ts` exports:
- `GRAPH` — all domain graph URIs (trailing slash canonical everywhere)
- `PREFIX` — individual prefix strings
- `COMMON_PREFIXES` — jb + dcterms + schema combined
- `graphFilter(domain)` — returns FILTER clause
- `domainFromGraph(uri)` — extracts domain from graph URI
- `POD_BASE` — the base URI

## Files to Refactor

### Priority 1 — Handlers with hardcoded URIs
1. **`ontology-view.handler.ts:6-15`** — replace 10 local `const *_GRAPH` with imports from `GRAPH`
2. **`sexuality.handler.ts`** — 6 hardcoded graph URIs → `GRAPH.sexuality.models`, `GRAPH.sexuality.volumes`
3. **`knowledge-graph.handler.ts:8`** — `POD_PREFIX` → import `POD_BASE`
4. **`codebase-graph.handler.ts:6`** — `GRAPH_PREFIX` → import `GRAPH.codebase`
5. **`music.handler.ts`** — replace inline PREFIX strings with `COMMON_PREFIXES`

### Priority 2 — Services with hardcoded URIs
6. **`search-index.service.ts:814,853-854,887,965,1003`** — hardcoded graph URIs → GRAPH imports
7. **`link-inference.service.ts:339,411`** — hardcoded graph URI → `GRAPH.links`, use `domainFromGraph()`
8. **`retrieval.service.ts:200,214`** — hardcoded base URI → `POD_BASE`, use `domainFromGraph()`

### Priority 3 — PREFIX duplication
All 8 files with `PREFIX jb:` strings → import `PREFIX.jb` or `COMMON_PREFIXES`

## Trailing Slash Fix (IMPORTANT)

Current code is inconsistent — some domains have trailing slash, some don't:
- `music/` ✓, `media/` ✓ → already correct
- `photos`, `books`, `notes`, `property`, `stories`, `values`, `practices`, `people` → missing trailing slash

The constants module normalizes ALL to trailing slash. STRSTARTS still works because `"photos/"` is a superset match of `"photos"`. But verify no SPARQL queries use exact graph match (`GRAPH <uri>`) for these — those would break. Check sexuality handler especially (uses exact `<...models.ttl>` matches which are fine, they're specific file URIs not domain prefixes).

## What NOT to refactor (separate concern)

- Filesystem I/O in music-pod.service.ts / photo-pod.service.ts — that's a bigger architectural discussion about the fs-as-source-of-truth pattern. Not in scope for #1258.
- SparqlService endpoint construction (`/pods/sparql` vs `/pods/query`) — leaving as-is, the service already centralizes this.

## AC

1. Zero hardcoded `localhost:3000/pods/jeff` strings remaining in handlers/services (except authorized-users.ts and service-token.middleware.ts which are identity URIs, not graph URIs)
2. All SPARQL prefix blocks use imports from sparql-constants
3. Tests pass, lint clean
4. No functional change — queries return same results
