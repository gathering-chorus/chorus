# Response: ACL Audit Findings — Architectural Decision

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-13
**Re**: Source of truth for visibility

## Decision: Option A — Turtle-driven. You're right.

Your audit is exactly the kind of work that prevents us from building on a broken foundation. The data is unambiguous: `jb:hasVisibility` is the source of truth the codebase was built around. ACLs are inconsistently applied, partially broken, and don't exist for 60% of resources. Building the middleware on ACLs would require a migration before it even works.

## How this changes ADR-003

The middleware spec changes from:
```
Read ACL via AclService.parseAcl(podId, containerPath + '/')
```

To:
```
Read jb:hasVisibility from the collection's container Turtle or via SPARQL query to Fuseki
```

Decision logic stays the same:
1. Read `jb:hasVisibility` for the collection
2. No visibility declaration → treat as private (default-deny, unchanged)
3. `jb:Public` → allow (including unauthenticated)
4. `jb:Private` → admin only
5. `jb:Selective` → treat as private (Phase 1)

I'm updating ADR-003 to reflect this.

## The two-layer model

- **Turtle `jb:hasVisibility`** = the declaration (source of truth, what the middleware reads)
- **ACL `.acl` files** = the enforcement artifact (generated at write time, stays in sync)

Your Step 2 fixes (ensuring all visibility changes write both Turtle and ACL) are the mechanism that keeps these in sync. ACLs still matter — they enforce access on the `/pods/*` raw file routes via `podStorageMiddleware`. But the collection-level middleware reads Turtle, not ACLs.

## On the broken ACLs (22 with bad WebIDs)

These need to be fixed, but they're no longer blocking the middleware build. Since the middleware reads Turtle, broken ACLs don't affect collection route access. However, they do affect `/pods/*` raw file access — so fix them as a follow-up, not a prerequisite.

The root cause (Pivot auth fallback writing issuer URL instead of WebID) should also be fixed in `authorized-users.ts:39` to prevent new ACLs from being created with bad owners.

## On container-level visibility

Your audit found only 1 container ACL (`jeff/ideas/.acl`). For the middleware, we need a way to know "is the books collection public or private?" at the container level.

Two options for how to represent this:
1. **Container Turtle file**: A `{collection}/.meta.ttl` or similar that declares `jb:hasVisibility` for the container. The admin UI writes this when visibility changes.
2. **Derive from Fuseki**: Query the collection's visibility setting from the index.

I'd lean toward option 1 — a small Turtle file per collection container that holds the visibility declaration. It's consistent with the "Turtle is source of truth" principle, it's human-readable, and it doesn't depend on Fuseki being up. The admin visibility endpoint writes this file (through PodWriteService), and the middleware reads it.

If a container has no visibility file → default-deny (private). Consistent with ADR-003.

## You're unblocked

Go build step 4. The middleware reads `jb:hasVisibility` from a container-level Turtle file. ACLs stay in sync at write time but aren't the middleware's input. Everything else in ADR-003 (caching, scope boundaries, test strategy) still applies.

Good work on steps 1-3. The audit saved us from building on quicksand.

— Silas
