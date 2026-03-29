# ACL File Audit — Step 3 Results

Date: 2026-02-13

## Inventory

Pod `jeff` (primary data pod):

| Collection | Resources | With ACL | Without ACL | Declared Visibility |
|-----------|-----------|----------|-------------|-------------------|
| Blog posts | 42 (+1 index) | 0 | 42 | All `jb:Public` |
| Books | 20 (+2 indices) | 20 | 0 | All `jb:Private` |
| Ideas | 3 | 2 resource + 1 container | 1 | All `jb:Private` |
| Projects | 1 | 1 | 0 | `jb:Private` |
| Property | 8 (property + houses + gardens + lands) | 0 | 8 | All `jb:Private` |
| Taxonomy | 2 | 0 | 2 | N/A (no visibility) |
| WordPress source | 1 | 0 | 1 | N/A |
| Admin | 1 | 0 | 1 | N/A |

Other pods (profile cards only): `solidcommunity`, `jeff-bridwell-personal`, `test-webid`, `jeffbridwell` — 4 profile card ACLs.

**Totals:** 27 `.acl` files across all pods. 55+ Turtle resources without ACLs.

---

## Issues Found

### 1. Broken owner WebId in 22 ACLs — HIGH

All 20 book ACLs and 2 profile card ACLs use:
```turtle
acl:agent <https://solidcommunity.net/>
```

This is the OIDC **issuer URL**, not a valid WebId. It comes from Pivot auth falling back to the issuer when no real WebId is available (`authorized-users.ts:39` even comments: "not a real WebID").

**Impact:** These ACLs grant owner Control to a URL that doesn't represent a person. The visibility middleware would not match Jeff's actual WebId (`https://jeffbridwell.solidcommunity.net/profile/card#me`) against these ACLs.

### 2. Missing ACLs for private resources — MEDIUM

- `ideas/consider-how-to-integrate-w-aws-serverless-for-sms-endpoint.ttl` — `jb:Private`, no ACL
- All 8 property resources (property.ttl, 1 house, 5 gardens, 1 land) — `jb:Private`, no ACLs
- `admin/users.ttl` — sensitive data, no ACL

These resources are only "private" because nothing grants public access. There's no ACL declaring private-only access either. The middleware would need to fall back to container/root ACL or treat missing-ACL as private-by-default.

### 3. No ACLs for public blog posts — LOW (design decision)

All 42 blog posts declare `jb:hasVisibility jb:Public` in Turtle but have zero ACL files.

This is fine IF the middleware reads visibility from the Turtle declaration (ADR-003's approach). But if the middleware checks ACLs, public resources would need explicit ACLs with `acl:agentClass foaf:Agent`.

### 4. Inconsistent container ACL coverage — MEDIUM

Only one container ACL exists: `jeff/ideas/.acl`. No container ACLs for:
- `books/`
- `projects/`
- `property/`
- `blog/posts/`

The container ACL fallback chain (which we just fixed the `.acl.acl` bug for) only works for the ideas container.

### 5. Three different owner WebIds — LOW

| WebId | Used in | Count |
|-------|---------|-------|
| `https://solidcommunity.net/` | Book ACLs, 2 profile ACLs | 22 |
| `https://jeffbridwell.solidcommunity.net/profile/card#me` | Ideas container, e2e artifacts | 3 |
| `https://jeff-bridwell-personal.solidcommunity.net/profile/card#me` | 1 idea resource ACL | 1 |

The second and third are both valid WebIds for Jeff (primary and alternate). The first is broken.

### 6. E2E test artifacts on disk — LOW

- `jeff/ideas/e2e-promote-idea-1770994044738.ttl` + `.acl`
- `jeff/projects/e2e-promote-idea-1770994044738.ttl` + `.acl`

Leftover from Playwright E2E test runs. Should be cleaned up or handled by test teardown.

---

## Migration Recommendations

### Before building visibility middleware (blockers)

1. **Fix Pivot auth WebId resolution** — Stop using the OIDC issuer URL as an ownerWebId. Either resolve to a proper WebId during auth, or skip ACL creation when only an issuer is available.

2. **Decide: ACL-driven or Turtle-driven visibility?**
   - **ACL-driven**: Middleware reads `.acl` files → requires generating ACLs for ALL resources
   - **Turtle-driven** (ADR-003's approach): Middleware reads `jb:hasVisibility` from Turtle → ACLs are enforcement layer, not discovery
   - **Recommendation**: Turtle-driven. The Turtle declaration is the source of truth. ACLs enforce it. This matches the existing data shape (42 public posts with no ACLs, private resources with no ACLs).

3. **Re-generate broken book ACLs** — Replace `https://solidcommunity.net/` with `https://jeffbridwell.solidcommunity.net/profile/card#me` in all 20 book ACLs. Can be a one-time migration script.

### After middleware is built (follow-up)

4. **Generate missing ACLs for private resources** — Property resources and the orphaned idea need ACLs for enforcement consistency.

5. **Add container ACLs** — Create default container `.acl` files for `books/`, `projects/`, `property/`, `blog/posts/` to enable proper fallback.

6. **Clean up E2E artifacts** — Add afterAll cleanup in Playwright tests to remove test-created resources.

7. **Normalize owner WebId** — Consolidate to single canonical WebId across all ACLs.
