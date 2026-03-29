# SPARQL Scoping Audit

**Date**: 2026-02-14
**Author**: Silas (Architect)
**Status**: Complete
**Context**: Jeff flagged that ADR-003 sections 7-8 (SPARQL horizontal access, ontology cross-collection traversal) are documented intent, not verified enforcement. This audit maps every SPARQL query path in the codebase.

---

## Key Finding

**Collection handlers don't query Fuseki today.** Books, blog, ideas, projects, property — they all read Turtle files directly from the filesystem. Fuseki is queried only through admin-only dashboard endpoints. The horizontal access risk is currently theoretical, not active.

This is good news for today. The risk materializes when:
1. A collection handler starts querying Fuseki (e.g., for search, for the AI layer)
2. The SPARQL dashboard is opened to non-admin users
3. New handlers are built for harvesters that need cross-resource queries

The scoping pattern must be established **before** any of those things happen.

---

## Complete Query Surface Inventory

### Hard-Coded SPARQL Queries (2 total)

Both live in `SparqlService.getStats()`:

| File | Line | Query | Scoped | Access |
|------|------|-------|--------|--------|
| `src/services/sparql.service.ts` | 317 | `SELECT (COUNT(*) as ?count) WHERE { GRAPH ?g { ?s ?p ?o } }` | No — counts all triples across all graphs | Admin only |
| `src/services/sparql.service.ts` | 322 | `SELECT (COUNT(DISTINCT ?g) as ?count) WHERE { GRAPH ?g { ?s ?p ?o } }` | No — counts all distinct graphs | Admin only |

These are aggregate stats (total triples, total graphs). They reveal scale info but not content. Acceptable for admin-only use.

### User-Submitted SPARQL (1 endpoint)

| Route | Handler | File | Line | Auth | CSRF |
|-------|---------|------|------|------|------|
| `POST /api/dashboard/sparql` | `executeQuery()` | `src/handlers/dashboard.handler.ts` | 165-220 | `apiAdminMiddleware` | Yes |

- Accepts **any SPARQL** — SELECT, CONSTRUCT, ASK, INSERT, DELETE, DROP, CLEAR
- No graph clause validation or scoping enforcement
- Admin can query across all pods and all named graphs
- Only query **type** is logged (line 207), not query **text**

### Data Sync Operations (properly scoped)

All FusekiSyncService operations use scoped graph URIs:

| Operation | Method | Graph URI Pattern | Scoped |
|-----------|--------|-------------------|--------|
| Load resource | `loadGraph()` | `http://localhost:3000/pods/{podId}/{path}` | Yes |
| Drop resource | `dropGraph()` | `http://localhost:3000/pods/{podId}/{path}` | Yes |
| Remove resource | `removeResource()` | `http://localhost:3000/pods/{podId}/{path}` | Yes |

### Dangerous Internal Method (no route)

| Method | File | Line | Query | Exposed |
|--------|------|------|-------|---------|
| `clearAll()` | `src/services/fuseki-sync.service.ts` | 406 | `DROP ALL` | No route — internal only |

This deletes the entire dataset. Not exposed via any endpoint, but exists in code. Should be marked with a prominent warning comment.

---

## Access Control Summary

| Query Path | Who Can Trigger | How Protected |
|------------|----------------|---------------|
| `getStats()` aggregate counts | Admin only | `adminMiddleware` / `apiAdminMiddleware` |
| User-submitted SPARQL | Admin only | `apiAdminMiddleware` + CSRF |
| Sync load/drop | Internal (FusekiSyncService) | No HTTP route — triggered by file operations |
| `DROP ALL` | Nobody (no route) | Internal method only |
| **Collection page/API handlers** | **Public/authenticated users** | **Don't query Fuseki — read from filesystem** |

---

## What This Means

### Current State: Safe

All Fuseki query paths are admin-only. Non-admin users (including unauthenticated visitors on public collections) never trigger a SPARQL query. The horizontal data access concern documented in ADR-003 section 7 is not an active vulnerability.

### Future State: Needs Enforcement

The risk activates when any of these happen:
- **Search feature**: A collection handler queries Fuseki for search results instead of scanning the filesystem
- **AI companion**: The thinking partner runs SPARQL to find cross-domain connections
- **Harvest dashboard**: A non-admin view shows harvested metadata via SPARQL
- **Collection enrichment**: Handlers query Fuseki for related resources across collections

When any of these are built, the handler's SPARQL must be scoped to the requesting user's accessible collections. The named graph URI convention (`http://localhost:3000/pods/{podId}/{collection}/...`) makes this straightforward — it's a `GRAPH` clause restriction.

---

## Recommended Actions

### Immediate (low effort, preventive)

1. **Add query text to audit log** — `dashboard.handler.ts` line 207 logs query type but not content. For a security-sensitive admin endpoint, log the full query. Quick fix.

2. **Add warning comment to `clearAll()`** — The `DROP ALL` internal method should have a prominent `// DANGER: Deletes entire Fuseki dataset` comment and ideally require a confirmation parameter.

3. **Document the scoping convention** — The named graph URI pattern (`http://localhost:3000/pods/{podId}/{path}`) is the mechanism for collection-level scoping. Document it as the canonical pattern for any future SPARQL query that serves non-admin users.

### When building non-admin query paths (before that work starts)

4. **Create a scoped query helper** — A function that takes a collection key (from `COLLECTION_TYPES`) and a user's accessible collections, and returns a SPARQL query wrapper that restricts `GRAPH` clauses to permitted named graph patterns. Makes the right thing easy.

   ```typescript
   // Example pattern
   function scopedQuery(sparql: string, collections: string[], podId: string): string {
     const graphPatterns = collections
       .map(c => `GRAPH <http://localhost:3000/pods/${podId}/${c}/>`)
       .join(' UNION ');
     // Wrap or validate the query to use only these graphs
   }
   ```

5. **Add a test that catches unscoped queries** — A lint rule or test that fails if a collection handler (non-admin route) calls `SparqlService.query()` without using the scoped helper. Prevents accidental unscoped queries in future handlers.

### When building the AI companion (prerequisite)

6. **Query scope from user ACLs** — The AI receives a "permitted graphs" list derived from the requesting user's visibility access. It can only reason over data in those graphs. Named graphs make this enforceable.

---

## Named Graph Convention Reference

```
http://localhost:3000/pods/{podId}/{containerPath}/{resource}.ttl

Examples:
  http://localhost:3000/pods/jeff/books/total-tai-chi.matthew-rochford.ttl
  http://localhost:3000/pods/jeff/blog/posts/hello-world.ttl
  http://localhost:3000/pods/jeff/ideas/startup-idea.ttl
  http://localhost:3000/pods/jeff/property/house.ttl

Collection-level GRAPH pattern:
  GRAPH ?g { ... } FILTER(STRSTARTS(STR(?g), "http://localhost:3000/pods/jeff/books/"))
```

This pattern enables:
- **Pod-level scoping**: Restrict to one user's data
- **Collection-level scoping**: Restrict to one collection within a pod
- **Resource-level scoping**: Target a specific resource's graph
