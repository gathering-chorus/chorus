# ADR-003: Visibility Enforcement Architecture

**Date**: 2026-02-13
**Status**: Accepted
**Deciders**: Jeff Bridwell, Wren (PM), Silas (Architect), Kade (Engineer)
**Meeting**: `../meetings/2026-02-13-visibility-enforcement-gap.md`

## Context

E2E testing (Phase 4, 60 tests) exposed that the collection visibility system stores ACL files but never enforces them on collection routes. Two disconnected systems exist:

1. **Express middleware** (`adminMiddleware` / `apiAdminMiddleware`) — binary admin-or-blocked. Visibility ACLs never consulted.
2. **WAC ACL system** (`AclService`) — `.acl` files are written and parsed, but `UserAccessService` is read-only reporting, never used as middleware.

Result: an admin sets a collection to "public," the `.acl` is written with `foaf:Agent` read access, but a regular user hitting the route still gets 403 because middleware blocks before the handler runs. The graduation model (private → shared → public) is architecturally present but not enforced.

## Decision

### 1. Public means unauthenticated access

Collections set to "public" are accessible to any visitor, including unauthenticated users. No login required. This is the whole point of the graduation model — "publish" means publish.

### 2. Selective treated as private (for now)

Selective visibility (per-agent, per-group WAC checks) is deferred to a follow-up. For this phase, selective falls back to private (admin-only). Rationale: the testing surface for a security boundary with agent/group matching is not yet sufficient to ship with confidence. Private + public first; selective when test coverage can prove it works.

### 3. Enforcement on both HTML and API routes

Visibility enforcement applies consistently to both `/collection/*` HTML pages and their corresponding `/api/*` JSON endpoints. A user who can see the page gets the data. No inconsistency between layers.

### 4. Source of truth: Turtle declarations, not ACL files

**Updated 2026-02-13 based on Kade's Step 3 audit findings.**

Kade's audit of existing pod data revealed:
- 60% of resources have no `.acl` file at all
- 22 ACLs have broken owner WebIDs (OIDC issuer URL, not a valid WebID)
- Only 1 container ACL exists (`jeff/ideas/.acl`)
- All 42 blog posts declare `jb:hasVisibility jb:Public` in Turtle but have no ACLs

The codebase was built with `jb:hasVisibility` as the visibility declaration from day one. ACLs were added later and inconsistently. Building the middleware on ACLs would require a migration before it even works.

**Decision**: Two-layer model:
- **Turtle `jb:hasVisibility`** = the declaration (source of truth, what the middleware reads)
- **ACL `.acl` files** = the enforcement artifact (generated at write time, stays in sync via PodWriteService)

Collection-level visibility is stored in a container Turtle file (e.g., `books/.meta.ttl`) with a `jb:hasVisibility` triple. The admin visibility endpoint writes this file through PodWriteService, which also generates the corresponding ACL for `/pods/*` raw file access.

### 5. New middleware: `collectionVisibilityMiddleware`

A middleware factory replaces `adminMiddleware` / `apiAdminMiddleware` on collection routes only.

**Signature**: `collectionVisibilityMiddleware(collectionKey: string)`

**Decision logic**:
1. Read `jb:hasVisibility` from the collection's container Turtle file using `COLLECTION_TYPES` config
2. No visibility declaration exists → treat as private (default-deny)
3. `jb:Public` → allow (including unauthenticated)
4. `jb:Private` → admin only
5. `jb:Selective` → treat as private (Phase 1; wired to full WAC in Phase 2)

**Pattern**: Builds on `optionalAuth` (populate session without blocking), not on `adminMiddleware` (which redirects/blocks unauthenticated users). This is critical — public access requires a middleware shape that works without a session.

### 6. Visibility caching

Parsed visibility declarations cached in memory with a short TTL (15-30 seconds), invalidated on write via PodWriteService. Avoids per-request filesystem reads for data that changes infrequently. Cache TTL is a documented trade-off — see Test Strategy section.

### 7. SPARQL access stays admin-only (horizontal data access concern)

Visibility enforcement at the Express route layer does not protect the SPARQL query layer. Fuseki holds a **copy** of all pod data in named graphs (`http://localhost:3000/pods/{podId}/{path}`). A SPARQL query can reach across all named graphs regardless of collection visibility.

**Current state**: The dashboard SPARQL tool (`/api/dashboard/sparql`) is behind `apiAdminMiddleware`. Fuseki itself is on port 3030 (localhost only). No public SPARQL endpoint exists.

**Audit (2026-02-14)**: Full SPARQL scoping audit confirmed that **collection handlers do not query Fuseki**. All collection pages and APIs read Turtle files from the filesystem. The entire SPARQL query surface is admin-only (2 hard-coded aggregate stats queries + 1 user-submitted SPARQL endpoint, all behind `apiAdminMiddleware`). The horizontal access concern is a future risk, not an active vulnerability. See `sparql-scoping-audit.md` for full inventory.

**Decision**: SPARQL access remains admin-only. No changes needed for Phase 1. But three guardrails apply:

1. **When non-admin SPARQL paths are built** (search, AI companion, harvest dashboard), handlers must scope queries to their collection's named graphs using the `GRAPH` clause with the URI convention: `http://localhost:3000/pods/{podId}/{containerPath}/`. A scoped query helper will be provided before that work starts.

2. **Fuseki port must not be exposed beyond localhost.** If the system ever moves to production hosting, Fuseki must be behind the Docker network only — no external port binding.

3. **Admin SPARQL endpoint should log full query text** for audit trail (currently logs type only).

**Future phases**:
- When selective ships (Phase 2): handlers should accept user context and scope SPARQL queries to graphs the user has access to.
- When the AI layer ships: the AI must receive a "query scope" based on the requesting user's ACLs. It can only reason over data the user is authorized to see. Named graphs make this enforceable — restrict the AI's SPARQL to permitted graphs.

### 8. Ontology traversal across visibility boundaries

The OWL ontology defines object properties that traverse collection boundaries. Every traversable relationship is a potential visibility leak when SPARQL is involved.

**Cross-collection relationships in the ontology:**

- **Books → Property**: `Book → onShelf → Shelf → inBookcase → Bookcase → inRoom → Room → inHouse → House → onProperty → Property`. A SPARQL query following this chain from a public books collection reaches into potentially private property data.
- **Ideas ↔ Projects**: `promotedTo`, `promotedFrom`, `mergedInto`. Two separate collections linked by object properties.
- **Profile → all collections**: `hasCollection` links profile to every collection. Traversal from a public profile could expose metadata about private collections.
- **AI-ready properties**: `relatedTo`, `mentions` — generic cross-domain links designed for discovery. Every one crosses potential visibility boundaries.

**The tension**: The ontology's value comes from cross-domain connections. The visibility model says different domains have different access levels. These are structurally in conflict when someone can write SPARQL or when the AI follows relationships.

**Phase 1 approach**: Collection-scoped queries. Handlers restrict SPARQL to their collection's named graphs. Cross-collection relationships exist in the Turtle files but aren't followed for non-admin users. Simple, safe.

**Architectural target (Phase 2+)**: Opaque URI pattern, aligned with how Linked Data works. Cross-collection references are returned as URIs but not resolved for users who lack access to the target collection. "This book references a shelf" — the URI is visible, but following it requires property collection access. URIs are identifiers, not guaranteed access.

**For the AI layer (Phase 3+)**: Visibility-aware query scoping. The AI can follow relationships, but is constrained to named graphs the requesting user can access. If books is public and property is private, the AI sees books data but `onShelf` targets are filtered out of its context.

**SHACL implication**: Current shapes are meta-level (ontology quality). As SHACL is expanded to validate instance data, shapes must not create implicit cross-collection dependencies. A shape requiring `jb:onShelf` on every Book would create a dependency on the property collection — validation would behave differently based on the viewer's access level. SHACL shapes should be collection-scoped: a Book shape validates book properties, not cross-collection references.

### 9. Scope boundaries — what doesn't change

These routes stay behind `adminMiddleware` / `apiAdminMiddleware` with no visibility logic:
- `/api/admin/*` — user management
- `/api/visibility` — changing visibility settings
- `/dashboard` — admin tool (including SPARQL query tool)
- `/admin/*` — admin pages

Only routes mapped to `COLLECTION_TYPES` (blog, books, property, ideas, projects) get the new middleware.

## Rationale

- **Single decision point**: One middleware, one place where visibility is evaluated. Easier to audit, test, and reason about than scattering ACL checks across handlers.
- **UserAccessService stays as reporting**: It iterates all resources and computes group memberships — too heavy for per-request gating. The middleware does a focused check on one collection's ACL.
- **Cache prevents I/O creep**: ACL files live on the filesystem. Without caching, every collection request becomes a disk read. At current scale this is invisible; at scale it becomes a bottleneck. Solve it now while the cost is low.
- **Phased selective**: Shipping a security boundary you can't verify with tests is shipping a hole. The agent/group matching logic exists in `UserAccessService.checkResourceAccess()` and can be wired in when test coverage supports it.

## Test Strategy

This section specifies the test categories required before the middleware ships. The ADR's own principle — "don't ship a security boundary you can't verify" — applies to its own implementation.

### Prerequisites (before middleware build begins)

**ACL service coverage (target: 90%+ on `acl.service.ts`)**
- `parseAcl()`: Valid public/private/selective ACLs, malformed files, missing files, empty files, unexpected permission combinations
- `checkAccess()`: Each visibility level × each user type (admin, authenticated non-admin, unauthenticated)
- Edge cases: `.acl` file exists but is unreadable, directory-level vs file-level ACLs, ACL references nonexistent group

**Write path audit**
- Verify all `.acl` write operations funnel through `PodWriteService`
- Identify and fix any writers that bypass the choke point (Kade flagged at least 5 files touching ACLs)
- Test: write ACL through each path → verify cache invalidation fires

**Migration audit**
- Script or manual check: for each collection in `COLLECTION_TYPES`, read the existing `.acl` file and verify it matches the admin's intended visibility
- Document any mismatches and correct before middleware goes live

### Middleware tests (built alongside the middleware)

**Unit tests (mock AclService)**
- Public ACL + no auth → 200 (allow)
- Public ACL + authenticated user → 200 (allow)
- Private ACL + admin → 200 (allow)
- Private ACL + authenticated non-admin → 403
- Private ACL + no auth → 401/redirect
- Selective ACL → treated as private (same as private tests)
- No ACL file → treated as private (default-deny)
- Malformed ACL file → treated as private (fail-closed)
- Cache hit path (second request within TTL uses cached result)
- Cache invalidation path (write occurs, next request reads fresh)

**Integration tests (real AclService + filesystem)**
- Middleware + real `.acl` files on disk → correct access decisions
- Full read path: filesystem → AclService.parseAcl() → middleware decision → response
- Visibility change via API → cache invalidation → next request reflects new state

**E2E tests (Playwright)**
- Unauthenticated visitor hits public collection page → 200 with content
- Unauthenticated visitor hits public collection API → 200 with JSON
- Unauthenticated visitor hits private collection page → redirect to login
- Unauthenticated visitor hits private collection API → 401
- Admin hits any collection → 200 regardless of visibility
- Admin changes collection from private to public → unauthenticated visitor now gets 200
- Admin changes collection from public to private → unauthenticated visitor now gets redirect/401

**Negative/edge case tests**
- Authenticated non-admin user on a private collection → 403
- Missing `.acl` file (deleted from disk) → default-deny
- Race condition: visibility change during request (acceptable: TTL window documented as trade-off)

### Cache TTL as documented trade-off

The ACL cache TTL (recommended: 15-30 seconds) means a revoked-public collection may remain accessible for up to the TTL window if cache invalidation fails. This is acceptable for a personal site at current scale. The primary protection is write-path invalidation; TTL is the fallback. If invalidation misses a write path, the TTL is the exposure window.

## Consequences

- Collection routes become accessible to non-admin users (and unauthenticated visitors for public collections) — this is the intended behavior but is a meaningful change in the app's access surface
- ACL files become load-bearing for access control, not just metadata — correctness and existence of `.acl` files matters more
- Default-deny (no ACL = private) protects against missing `.acl` files
- Future work: Phase 2 adds selective enforcement (per-agent, per-group WAC checks) with dedicated test coverage
- Handlers become responsible for scoping their Fuseki queries to collection-relevant named graphs — route-level gating + query-level scoping = defense in depth
- Fuseki port (3030) must remain localhost-only; never exposed externally
- Future work: AI context layer must receive a "query scope" derived from the requesting user's ACLs — it can only reason over data in permitted named graphs

## Data Access Paths (for reference)

| Path | Gated By | Visibility-Aware? |
|------|----------|-------------------|
| `/collection/*` HTML routes | `collectionVisibilityMiddleware` (new) | Yes — Phase 1 |
| `/api/*` collection JSON routes | `collectionVisibilityMiddleware` (new) | Yes — Phase 1 |
| `/api/dashboard/sparql` | `apiAdminMiddleware` (admin-only) | No — admin sees all |
| `/pods/*` raw file access | `adminMiddleware` + `podStorageMiddleware` (ACL-enforced) | Yes — existing |
| Fuseki port 3030 direct | Localhost network only | No — must not be exposed |
| Future AI SPARQL queries | TBD — query scope from user ACLs | Phase 2+ |

## Key Files

- `src/middlewares/auth.middleware.ts` — existing admin middleware (to be augmented, not replaced globally)
- `src/services/acl.service.ts` — ACL parsing (`parseAcl`, `checkAccess`)
- `src/services/user-access.service.ts` — reporting service (not middleware)
- `src/config/collection-types.ts` — the 5 collection types and their container paths
- `src/services/pod-write.service.ts` — write choke point (cache invalidation hook)
- `src/middleware/pod-storage.middleware.ts` — existing ACL enforcement on `/pods/*` (reference pattern)
- `src/app.ts` — route definitions where middleware is assigned 