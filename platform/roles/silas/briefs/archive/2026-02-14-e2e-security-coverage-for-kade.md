# Brief: E2E Security Coverage — Close the Blind Spots

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: High — security-critical gaps in test coverage
**Context**: Wren mapped all 90 access control permutations (`product-manager/access-control-permutation-matrix.md`). I audited the middleware architecture. Current E2E coverage is **~39%**. The gaps are concentrated in write denial and visibility transitions — the highest-risk areas.

---

## The Problem

We have strong middleware (`collectionVisibilityMiddleware`, `apiAdminMiddleware`) but **no E2E tests proving write operations are denied for non-admin users**. If either middleware breaks, any authenticated user — or unauthenticated visitor on public collections — could modify pod data. The middleware is the only thing standing between the internet and Jeff's knowledge graph.

---

## Sprint 1: Write Denial Tests (CRITICAL — do first)

**10 new tests.** For each collection with API write routes (Books, Property, Ideas, Projects), verify that non-admin users get rejected.

Blog has no API write routes — skip it.

### Test Pattern

```typescript
// For each collection: Books, Property, Ideas, Projects
describe('write denial - {collection}', () => {
  test('unauthenticated POST → 401', async () => {
    // No auth headers
    const res = await request(app).post('/api/{collection}').send({ title: 'test' });
    expect(res.status).toBe(401);
  });

  test('authenticated non-admin POST → 403', async () => {
    // Auth headers for non-admin user
    const res = await request(app)
      .post('/api/{collection}')
      .set('Authorization', `Bearer ${nonAdminToken}`)
      .send({ title: 'test' });
    expect(res.status).toBe(403);
  });
});
```

Also test DELETE and PUT where they exist. The middleware is `apiAdminMiddleware` — you're proving it works on every write route.

### Additional write denial tests

| Test | Endpoint | Expected |
|------|----------|----------|
| Unauth write on public collection (Blog is public) | POST `/api/harvest` | 401 (public read ≠ public write) |
| Non-admin write on public collection | POST `/api/harvest` | 403 |

**Total Sprint 1: 10 tests**

---

## Sprint 2: Visibility Transition Tests (HIGH)

**15 new tests.** Toggle each collection's visibility and verify all 3 user types get the correct response.

### Important: Cache TTL

The visibility middleware caches `.meta.ttl` reads with a **30-second TTL**. Between visibility toggles in tests, you need to either:
- Invalidate the cache (add a test helper that calls the cache clear method)
- Wait 30+ seconds (bad for test speed)

**Recommendation**: Add a `clearVisibilityCache()` test helper. The cache is in `collection-visibility.middleware.ts`.

### Test Pattern

For each collection (Blog, Books, Ideas, Projects, Property):

```typescript
describe('visibility transition - {collection}', () => {
  test('toggle to private → unauth blocked', async () => {
    // Write jb:Private to .meta.ttl
    // Clear visibility cache
    // Request as unauth → expect 302 (HTML) or 401 (API)
  });

  test('toggle to public → unauth allowed', async () => {
    // Write jb:Public to .meta.ttl
    // Clear visibility cache
    // Request as unauth → expect 200
  });

  test('toggle back → verify restored', async () => {
    // Restore original visibility
    // Clear visibility cache
    // Verify original behavior
  });
});
```

**Total Sprint 2: 15 tests**

---

## Sprint 3: Structural Gaps (MEDIUM)

### Ideas/Projects HTML Coupling (3 tests)

Ideas and Projects share `/collection/ideas`, gated by `ideasVisibility` only. Test:

| Test | Setup | Expected |
|------|-------|----------|
| Ideas private → projects data hidden on HTML page | Set ideas to private | Unauth can't see `/collection/ideas` (which also shows projects) |
| Projects API independent of ideas visibility | Set ideas to private, projects to public | `/api/projects` still accessible, `/api/ideas` blocked |
| Verify no project data leaks in private ideas page | Set ideas to private | Admin sees page; verify project data is present for admin |

### Default-Deny (2 tests)

| Test | Setup | Expected |
|------|-------|----------|
| Missing `.meta.ttl` → private | Delete `.meta.ttl` from a collection | Unauth blocked, admin allowed |
| Malformed `.meta.ttl` → private | Write garbage to `.meta.ttl` | Unauth blocked, admin allowed |

### Selective-as-Private (5 tests)

For each collection, set visibility to `jb:Selective` and verify it behaves identically to `jb:Private` (admin only). One test per collection.

### Home Page Leakage (1 test)

| Test | Setup | Expected |
|------|-------|----------|
| Home page doesn't expose private collection data | All collections private | Unauth visits `/` → no titles/summaries from private collections visible |

### CSRF on Visibility Changes (1 test)

| Test | Setup | Expected |
|------|-------|----------|
| Visibility change without CSRF token | Admin tries to modify `.meta.ttl` without CSRF | Request rejected |

**Total Sprint 3: 12 tests**

---

## Summary

| Sprint | Focus | Tests | Priority |
|--------|-------|-------|----------|
| 1 | Write denial | 10 | CRITICAL |
| 2 | Visibility transitions | 15 | HIGH |
| 3 | Structural gaps | 12 | MEDIUM |
| **Total** | | **37** | |

After all three sprints: coverage goes from **~39% → ~95%** of the permutation space.

### Key Files

- Visibility middleware: `src/middleware/collection-visibility.middleware.ts` (cache TTL here)
- Admin middleware: `src/middlewares/auth.middleware.ts` (`apiAdminMiddleware`)
- Route registration: `src/app.ts` (lines 449-745)
- Existing E2E tests: `tests/e2e/` (extend these)
- Wren's full matrix: `../product-manager/access-control-permutation-matrix.md`
- Silas's architectural review: `../product-manager/briefs/2026-02-14-access-control-matrix-architectural-review.md`

### Reference: Route → Middleware Map

| Route | Visibility MW | Admin MW |
|-------|:---:|:---:|
| `/collection/blog` | blogVisibility | — |
| `/collection/books` | booksVisibility | — |
| `/collection/property` | propertyVisibility | — |
| `/collection/ideas` | ideasVisibility | — |
| `/api/books` (read) | apiBooksVisibility | — |
| `/api/books` (write) | — | apiAdminMiddleware |
| `/api/property` (read) | apiPropertyVisibility | — |
| `/api/property` (write) | — | apiAdminMiddleware |
| `/api/ideas` (read) | apiIdeasVisibility | — |
| `/api/ideas` (write) | — | apiAdminMiddleware |
| `/api/projects` (read) | apiProjectsVisibility | — |
| `/api/projects` (write) | — | apiAdminMiddleware |
| `/` (home) | — | — |

— Silas
