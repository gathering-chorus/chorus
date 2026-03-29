# Brief: SPARQL Scoping Audit — Results & Reduced Scope

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: Medium (downgraded from High) — preventive, not a fix
**Supersedes**: `2026-02-13-sparql-scoping-audit-for-kade.md`

## Good News

I completed the full audit. **Collection handlers don't query Fuseki.** Books, blog, ideas, projects, property — they all read Turtle files from the filesystem. The entire SPARQL query surface is admin-only (dashboard endpoints behind `apiAdminMiddleware`).

The horizontal data access concern from ADR-003 section 7 is not an active vulnerability. It's a future risk that activates when a non-admin query path is built (search, AI companion, harvest dashboard).

Full audit at `../sparql-scoping-audit.md`.

## What's Actually There

- **2 hard-coded queries** — both in `SparqlService.getStats()`, both aggregate counts (total triples, total graphs). Admin-only. Acceptable.
- **1 user-submitted SPARQL endpoint** — `POST /api/dashboard/sparql`. Admin-only + CSRF. No query validation or scoping, but admin can see everything anyway.
- **Sync operations** — `loadGraph()` and `dropGraph()` are all properly scoped to `{podId}/{path}` graph URIs. Clean.
- **1 dangerous internal method** — `clearAll()` runs `DROP ALL`. No route exposes it. Needs a warning comment.

## Three Small Tasks (when convenient)

These aren't urgent but they're clean-up from the audit:

### 1. Add query text to audit log
`dashboard.handler.ts` line 207 logs query *type* but not query *text*. For an admin endpoint that accepts arbitrary SPARQL, log the full query. Helps with debugging and security audit trail.

### 2. Add warning comment to `clearAll()`
`fuseki-sync.service.ts` line 406 has `this.sparqlService.update('DROP ALL')`. Add a prominent comment: `// DANGER: Deletes entire Fuseki dataset. Internal only — never expose via route.`

### 3. Document the scoped query pattern
The named graph URI convention (`http://localhost:3000/pods/{podId}/{path}`) is the collection-scoping mechanism. Add a brief comment block in `sparql.service.ts` or the README documenting this convention and the pattern for scoping queries:

```sparql
-- Scope to books collection:
GRAPH ?g { ... } FILTER(STRSTARTS(STR(?g), "http://localhost:3000/pods/jeff/books/"))
```

## What I Owe You (before non-admin SPARQL is built)

When the time comes to build search, the AI companion, or any handler that queries Fuseki for non-admin users, I'll provide:
- A scoped query helper design (wraps `GRAPH` clauses to permitted collections)
- A test pattern that catches unscoped queries in collection handlers

This is preventive architecture — establishing the pattern before it's needed. No rush until that work starts.

— Silas
