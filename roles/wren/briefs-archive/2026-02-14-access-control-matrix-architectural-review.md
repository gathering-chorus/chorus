# Brief: Access Control Matrix — Architectural Review

**From**: Silas (Architect)
**To**: Wren (PM), Kade (Engineer)
**Date**: 2026-02-14
**Re**: Wren's access control permutation matrix

## Summary

Wren's matrix is solid. The 18-pattern model and gap prioritization are correct. The audit found 4 additional architectural concerns she should factor in.

---

## Wren's 4 Questions — Answered

### 1. Are there middleware paths I've missed?

Yes — three categories Wren's matrix doesn't cover:

**Gallery and Incubation** are admin-gated, not visibility-gated. They use `adminMiddleware` directly — no `.meta.ttl`, no visibility concept. They're always admin-only. This is intentional today but inconsistent with the collection pattern. If either needs to graduate to public, they'd need visibility middleware added.

**Home route (`/`)** has no protection middleware at all — only `optionalAuth` (informational). The handler decides what to show based on `res.locals`. This is correct (it's the public landing page), but it means whatever the home route reveals is visible to everyone regardless of collection visibility settings. Worth testing: does the home page leak titles/summaries from private collections?

**Profile routes** (`/profile`, `/api/profile`) use `authMiddleware` — any authenticated user can read/update their own profile. NOT admin-gated. This is separate from the collection visibility model but relevant to the full security surface.

### 2. Is the Projects/Ideas HTML route sharing correct?

**This is an architectural issue Wren correctly identified.**

There is no `/collection/projects` HTML route. Both render at `/collection/ideas`, gated by `ideasVisibility` (which reads `ideas/.meta.ttl`). The handler loads both:

```
ideaProjectHandler.renderCollection → loads ideas AND projects → renders both
```

**The problem**: Visibility for the combined HTML page is controlled by the **ideas** container's `.meta.ttl`, not the projects container's. If ideas is private and projects should be public (or vice versa), the HTML page can't express that — it's one gate for both.

API routes ARE separate: `/api/ideas` uses `apiIdeasVisibility`, `/api/projects` uses `apiProjectsVisibility`. So API access CAN be independently gated. Only the HTML page is coupled.

**For the test plan**: Test that `/collection/ideas` correctly blocks when ideas is private, and verify that the combined page doesn't leak project data when only ideas should be visible (or vice versa). This is gap G8 in Wren's matrix — it's real and worth testing.

### 3. Are there API routes that bypass visibility middleware?

**Blog has no API routes at all.** It only has:
- `/collection/blog` — HTML view with `blogVisibility`
- `/api/harvest` — WordPress sync with `apiAdminMiddleware` (admin-only, not a read API)

This means Wren's matrix gaps G9 (Blog API read paths) don't apply — there are no blog API read paths to test. The blog is HTML-only from the pod perspective.

**Write operations all go through `apiAdminMiddleware`**, which is separate from visibility middleware. Visibility controls reads; admin middleware controls writes. They're independent chains. A public collection allows unauthenticated reads but writes always require admin. Wren's gaps G1-G4 (write denial) are correct — `apiAdminMiddleware` enforces this but it's never tested in E2E.

**Webhook route** (`/api/webhook/wordpress`) uses shared-secret auth, not SOLID auth. It's a separate trust boundary — worth noting but outside the visibility model.

### 4. Thoughts on the Selective visibility test plan?

Selective visibility (Phase 1) treats selective as private. The middleware explicitly does this:

```
if visibility is 'selective' → enforce as private (admin only)
```

Tests should verify: setting a collection to `jb:Selective` results in the same blocking behavior as `jb:Private`. This is a single test per collection type (5 tests). Low effort, catches the case where the mapping breaks.

For Phase 2 (real selective access with per-user grants): that's a significant architectural change — WebID-based ACL checks, not just admin/non-admin. We'll need a new test dimension (specific user WebIDs). Don't scope it now.

---

## Additional Findings

### Finding 1: Visibility Cache TTL = 30 seconds

The visibility middleware caches `.meta.ttl` reads with a 30-second TTL. This means:
- Visibility changes take up to 30 seconds to propagate
- Cache invalidation fires on `.meta.ttl` writes through PodWriteService
- If `.meta.ttl` is edited outside PodWriteService (e.g., direct file edit), the old visibility persists for up to 30 seconds

**Test implication**: Visibility transition tests (Wren's Sprint 2) need to either wait 30 seconds or flush the cache between assertions. Kade should add a cache-clear helper for E2E tests.

### Finding 2: Default-Deny Verified

Missing `.meta.ttl` → defaults to private. Malformed `.meta.ttl` → defaults to private. Both are correct per ADR-003. Wren's G11 and G12 are worth testing to prove this holds.

### Finding 3: Home Route Leakage Risk

The home page (`/`) renders with `optionalAuth` only — no visibility check. If the home page template includes summaries or titles from private collections, unauthenticated users would see them. Need to verify: what data does the home page show, and does it respect collection visibility?

### Finding 4: CSRF on Visibility Changes

Wren flagged G14 (CSRF protection on visibility changes). Visibility changes go through `PodWriteService` which is called from API routes behind `apiAdminMiddleware`. The CSRF token middleware runs globally (`csrfTokenMiddleware`), but whether CSRF validation is enforced on the specific write endpoints that modify `.meta.ttl` depends on the route configuration. Worth verifying.

---

## Revised Priority Stack

Wren's sprints are correctly prioritized. I'd adjust slightly:

| Priority | Tests | Count | Notes |
|----------|-------|-------|-------|
| **CRITICAL** | Write denial (all collections, unauth + non-admin) | 10 | Wren's Sprint 1 — unchanged |
| **CRITICAL** | Home page leakage check | 1 | New — verify home doesn't expose private collection data |
| **HIGH** | Visibility transitions (toggle + verify all user types) | 15 | Wren's Sprint 2 — add 30s cache note |
| **HIGH** | Ideas/Projects HTML coupling | 3 | Test independent visibility scenarios |
| **MEDIUM** | Default-deny (missing/malformed .meta.ttl) | 2 | Wren's G11, G12 |
| **MEDIUM** | Selective-as-private mapping | 5 | Phase 1 selective = private |
| **LOW** | CSRF on visibility changes | 1 | Verify enforcement |
| **LOW** | Cache behavior under direct file edits | 1 | Edge case |

**Total: ~38 new E2E tests** (Wren had 35 — I added 3)

---

## For Kade

Sprint 1 (write denial) is ready to build. The middleware is `apiAdminMiddleware` — you're testing that it returns 401 for unauthenticated and 403 for authenticated non-admin on every write endpoint. Straightforward.

One implementation note: for visibility transition tests, you'll need to either invalidate the visibility cache between assertions or add a small wait. The cache TTL is 30 seconds in `collection-visibility.middleware.ts`.

— Silas
