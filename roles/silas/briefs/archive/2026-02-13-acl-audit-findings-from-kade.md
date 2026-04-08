# ACL Audit Findings — Architectural Decision Needed

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-02-13
**Re:** ADR-003 Visibility Enforcement — Step 3 audit results

## Context

I've completed steps 1-3 of the visibility enforcement implementation sequence from the meeting:

- **Step 1:** ACL service test coverage → 100%. Found and fixed a container ACL fallback bug (was looking for `.acl.acl` instead of `.acl`).
- **Step 2:** Write path audit → Found and fixed 2 bugs: (a) idea-project handler only updated ACL for selective visibility, not public/private changes; (b) pod-write.service deleted `.acl` files directly via `fs.unlinkSync` instead of routing through `AclService.deleteAcl()`. All 1577 tests passing.
- **Step 3:** Audited all `.acl` files on disk. Full results below.

## What I Found on Disk

### Inventory (pod `jeff`)

| Collection | Resources | With ACL | Without ACL | Declared Visibility |
|-----------|-----------|----------|-------------|-------------------|
| Blog posts | 42 | 0 | 42 | All `jb:Public` |
| Books | 20 | 20 | 0 | All `jb:Private` |
| Ideas | 3 | 2 resource + 1 container | 1 | All `jb:Private` |
| Projects | 1 | 1 | 0 | `jb:Private` |
| Property | 8 | 0 | 8 | All `jb:Private` |

### Critical Issues

1. **22 ACLs have broken owner WebId** — All 20 book ACLs + 2 profile card ACLs use `acl:agent <https://solidcommunity.net/>` — the OIDC issuer URL, not a valid WebId. Comes from Pivot auth fallback (`authorized-users.ts:39`). These ACLs grant owner Control to a URL that doesn't represent a person.

2. **Private resources with no ACLs** — Property resources (8 files) and 1 idea are `jb:Private` in Turtle but have zero `.acl` sidecars.

3. **Public blog posts with no ACLs** — All 42 blog posts declare `jb:hasVisibility jb:Public` in Turtle. None have ACL files granting `foaf:Agent` read access.

4. **Only 1 container ACL exists** — `jeff/ideas/.acl`. No container ACLs for `books/`, `projects/`, `property/`, `blog/posts/`.

## The Question for You

ADR-003 proposes `collectionVisibilityMiddleware` that filters resources by visibility. **What should be the source of truth for visibility?**

### Option A: Turtle-driven (my recommendation)

Read `jb:hasVisibility` from the Turtle resource files. ACLs become the enforcement layer (applied at write time) but not the discovery mechanism.

**Pros:**
- Matches existing data shape — 42 public posts work with no changes
- Single source of truth already exists in every resource
- No migration needed for the middleware to work
- Simpler middleware logic (parse one Turtle triple)

**Cons:**
- Two representations of visibility (Turtle + ACL) can drift
- SOLID spec purists would say ACL is the canonical access layer

### Option B: ACL-driven

Read `.acl` sidecar files to determine visibility. Presence of `foaf:Agent` read access = public. Owner-only = private. Named agents = selective.

**Pros:**
- Single representation of access control
- Aligns with SOLID/WAC spec intent

**Cons:**
- Requires generating ACLs for ALL 42 blog posts + 8 property resources + taxonomy/admin
- Requires fixing 22 broken ACLs first
- Requires fixing Pivot auth WebId resolution
- ACL parsing is heavier than reading a single Turtle triple
- Container ACL fallback adds complexity

### My Take

The data strongly argues for **Option A (Turtle-driven)**. The codebase was built with `jb:hasVisibility` as the visibility declaration from day one. ACLs were added later and inconsistently — 60% of resources have no ACL at all, and the ones that do have broken owners. Building the middleware on ACLs would require a significant migration before it even works.

I'd suggest: Turtle declaration is the source of truth. ACLs enforce it (we ensure they stay in sync at write time — which the Step 2 fixes already address). The middleware reads Turtle.

Waiting on your call before I build step 4.

— Kade
