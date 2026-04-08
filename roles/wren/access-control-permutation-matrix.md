# Access Control Permutation Matrix

**Authors**: Wren (PM) + Silas (Architect)
**Date**: 2026-02-14
**Purpose**: Map all positive/negative E2E test cases for security/ACL validation
**Board item**: #5

---

## Dimensions

| Dimension | Values |
|-----------|--------|
| **User type** | Unauthenticated, Authenticated (non-admin), Admin |
| **Visibility** | Public, Private |
| **Route type** | HTML page, API read, API write |
| **Collection** | Blog, Books, Ideas, Projects, Property |

**Total permutations**: 3 × 2 × 3 × 5 = **90**

---

## Current Visibility Settings

| Collection | Current State | .meta.ttl |
|------------|--------------|-----------|
| Blog | **Public** | `jb:Public` |
| Books | **Private** | `jb:Private` |
| Ideas | **Private** | `jb:Private` |
| Projects | **Private** | `jb:Private` |
| Property | **Private** | `jb:Private` |

---

## Expected Behavior Matrix

### When Collection is PUBLIC

| # | User | Route | Expected | HTTP Code | Notes |
|---|------|-------|----------|-----------|-------|
| 1 | Unauth | HTML page | **ALLOW** | 200 | Public content visible to anyone |
| 2 | Unauth | API read | **ALLOW** | 200 | Public data available via API |
| 3 | Unauth | API write | **DENY** | 401 | No auth = no write, ever |
| 4 | Auth non-admin | HTML page | **ALLOW** | 200 | Public + authenticated |
| 5 | Auth non-admin | API read | **ALLOW** | 200 | Public + authenticated |
| 6 | Auth non-admin | API write | **DENY** | 403 | Authenticated but not admin |
| 7 | Admin | HTML page | **ALLOW** | 200 | Full access |
| 8 | Admin | API read | **ALLOW** | 200 | Full access |
| 9 | Admin | API write | **ALLOW** | 200/201 | Full access |

### When Collection is PRIVATE

| # | User | Route | Expected | HTTP Code | Notes |
|---|------|-------|----------|-----------|-------|
| 10 | Unauth | HTML page | **DENY** | 302→/login | Redirect to login |
| 11 | Unauth | API read | **DENY** | 401 | Not authenticated |
| 12 | Unauth | API write | **DENY** | 401 | Not authenticated |
| 13 | Auth non-admin | HTML page | **DENY** | 403 | Authenticated but not authorized |
| 14 | Auth non-admin | API read | **DENY** | 403 | Authenticated but not authorized |
| 15 | Auth non-admin | API write | **DENY** | 403 | Authenticated but not admin |
| 16 | Admin | HTML page | **ALLOW** | 200 | Full access |
| 17 | Admin | API read | **ALLOW** | 200 | Full access |
| 18 | Admin | API write | **ALLOW** | 200/201 | Full access |

**18 unique behavioral patterns × 5 collections = 90 test cases**

---

## Current E2E Coverage

### What's Tested (from codebase analysis)

| Pattern | Blog (Public) | Books (Private) | Ideas (Private) | Projects (Private) | Property (Private) |
|---------|:---:|:---:|:---:|:---:|:---:|
| **#1** Unauth + HTML + Public | ✅ | — | — | — | — |
| **#2** Unauth + API read + Public | ❌ | — | — | — | — |
| **#3** Unauth + API write + Public | ❌ | — | — | — | — |
| **#4** Auth + HTML + Public | ✅ | — | — | — | — |
| **#5** Auth + API read + Public | ❌ | — | — | — | — |
| **#6** Auth + API write + Public | ❌ | — | — | — | — |
| **#7** Admin + HTML + Public | ✅ | — | — | — | — |
| **#8** Admin + API read + Public | ✅ | — | — | — | — |
| **#9** Admin + API write + Public | — | — | — | — | — |
| **#10** Unauth + HTML + Private | — | ✅ | ✅ | ❌ | ✅ |
| **#11** Unauth + API read + Private | — | ✅ | ✅ | ✅ | ✅ |
| **#12** Unauth + API write + Private | — | ❌ | ❌ | ❌ | ❌ |
| **#13** Auth + HTML + Private | — | ✅ | ✅ | ❌ | ✅ |
| **#14** Auth + API read + Private | — | ✅ | ✅ | ✅ | ✅ |
| **#15** Auth + API write + Private | — | ❌ | ❌ | ❌ | ❌ |
| **#16** Admin + HTML + Private | — | ✅ | ✅ | ❌ | ✅ |
| **#17** Admin + API read + Private | — | ✅ | ✅ | ✅ | ✅ |
| **#18** Admin + API write + Private | — | ✅ | ✅ | ✅ | ✅ |

**Legend**: ✅ = tested, ❌ = NOT tested, — = N/A (collection not in that visibility state currently)

---

## Gap Analysis

### CRITICAL — Security Gaps (write operations)

These are the highest-priority missing tests. Write rejection for non-admin users is enforced by `apiAdminMiddleware` but **never verified in E2E tests**:

| # | Gap | Risk |
|---|-----|------|
| G1 | **Unauth write attempt on ANY collection** → should get 401 | If middleware fails, unauthenticated users could modify data |
| G2 | **Auth non-admin write attempt on ANY collection** → should get 403 | If middleware fails, any logged-in user could modify data |
| G3 | **Unauth write on public collection** → should still get 401 | Public visibility should never grant write access |
| G4 | **Auth non-admin write on public collection** → should get 403 | Public read ≠ public write |

**Recommended test**: One test per collection × 2 user types = **10 new tests** minimum

### HIGH — Visibility State Transitions

Only tested for one collection (books private→public→private via dynamic change). Need:

| # | Gap | Risk |
|---|-----|------|
| G5 | **Blog set to Private** → unauth should be blocked | Blog is the only public collection; if it goes private, does the middleware work? |
| G6 | **Books/Ideas/Property set to Public** → unauth should see data | When Jeff graduates a collection, does public access actually work? |
| G7 | **Projects set to Public** → needs HTML page test too | Projects has no HTML collection page — is that intentional? |

**Recommended test**: Toggle each collection's visibility and verify all 3 user types. **~15 new tests**

### MEDIUM — Collection-Specific Gaps

| # | Gap | Notes |
|---|-----|-------|
| G8 | **Projects has no HTML collection page** | Ideas + Projects share `/collection/ideas` — is project visibility tested via that route? |
| G9 | **Blog has no API read routes** | Blog is WordPress-driven; API reads go through collection handler. Is the API path tested? |
| G10 | **Per-item visibility (Ideas)** | Ideas support per-item visibility — only partially tested |

### LOW — Edge Cases

| # | Gap | Notes |
|---|-----|-------|
| G11 | Malformed .meta.ttl → should default to private | Defensive testing |
| G12 | Missing .meta.ttl → should default to private | Defensive testing |
| G13 | Visibility cache expiry behavior | Performance/correctness |
| G14 | CSRF protection on visibility changes | Security hardening |
| G15 | Selective visibility (Phase 1) | Not implemented yet — future |

---

## Recommended New Tests — Prioritized

### Sprint 1: Write Operation Denial (10 tests, CRITICAL)

For each collection (Blog, Books, Ideas, Projects, Property):
- `test: unauthenticated POST/PUT/DELETE → 401`
- `test: authenticated non-admin POST/PUT/DELETE → 403`

### Sprint 2: Visibility Transitions (15 tests, HIGH)

For each collection:
- Toggle visibility to opposite state
- Verify all 3 user types get correct response
- Toggle back and verify restored

### Sprint 3: Cross-Collection + Edge Cases (10 tests, MEDIUM)

- Projects HTML access through shared Ideas page
- Blog API read paths
- Per-item visibility on Ideas (create private item, verify non-admin can't read)
- Malformed/missing .meta.ttl defaults to private
- CSRF on visibility change endpoints

### Total: ~35 new E2E tests to achieve full coverage

---

## Coverage Summary

| Category | Current | Needed | Gap |
|----------|---------|--------|-----|
| Read access (private collections) | 16/20 | 20/20 | 4 (Projects HTML) |
| Read access (public collections) | 4/9 | 9/9 | 5 (Blog API paths) |
| Write denial (non-admin) | 0/10 | 10/10 | **10 (CRITICAL)** |
| Visibility transitions | 3/15 | 15/15 | 12 |
| Edge cases | 0/5 | 5/5 | 5 |
| **Total** | **23/59** | **59/59** | **36 tests** |

Current effective coverage: **~39%** of the permutation space.

---

## For Silas

Please review this matrix from the architecture side:
1. Are there middleware paths I've missed?
2. Is the Projects/Ideas HTML route sharing correct?
3. Are there API routes that bypass visibility middleware?
4. Any thoughts on the Selective visibility test plan for Phase 1?

## For Kade

This becomes the test plan. Sprint 1 (write denial) is the priority — these are security-critical gaps. The 10 write denial tests should be written first.

— Wren
