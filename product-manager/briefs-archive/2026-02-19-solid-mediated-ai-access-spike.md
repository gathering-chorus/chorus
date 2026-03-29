# Spike: SOLID-Mediated AI Access

**From:** Wren
**Date:** 2026-02-19
**Type:** Discovery spike (time-boxed research + proof of concept)
**Refs:** security-trust-model.md, DEC-026, Jeff's patent US9552400B2

## The Insight

Jeff's original architecture puts AI *beneath* the pod and ontology — the pod is sovereign, the AI is a client. Today's reality inverts that: Claude reads the filesystem directly, accumulates context in MEMORY.md, and Anthropic sees everything in sessions. The classification system (hooks, scrubbers) is a patch on the wrong layer.

**The question:** Can we make AI roles interact with Jeff's data through the SOLID pod's ACL model instead of direct filesystem access?

## What We Found

### The App Already Has Most of This

The Express app at `jeff-bridwell-personal-site/` implements:

| Component | Status | Location |
|-----------|--------|----------|
| Pod HTTP API | Working | `/api/pods/:podId/:resourcePath` (GET/PUT) |
| WAC ACL files | Working | `.acl` Turtle files alongside resources |
| ACL enforcement | Working | `acl.service.ts` — `checkAccess(podId, resourcePath, webId, requiredMode)` |
| Content negotiation | Working | Returns Turtle, JSON-LD, N-Triples |
| SPARQL endpoint | Working | Fuseki at `localhost:3030/pods/sparql` |
| WebID profiles | Minimal | `data/pods/{podId}/profile/card.ttl` |
| Agent/service auth | **Missing** | All endpoints behind session-based `apiAdminMiddleware` |

**The gap is one layer: service authentication for non-human clients.** Everything else exists.

### What the SOLID Spec Says About AI Agents

- WAC ACLs make **no distinction between human and non-human agents**. A WebID is a WebID.
- CSS (Community Solid Server) has client credentials for headless auth, but it's non-standard.
- **No existing project combines SOLID pods with LLM agents.** This would be novel.
- The Solid-OIDC spec only covers interactive browser auth. Server-side client auth is an open issue (solid/specification#504).

### What We'd Build (Not CSS — Our Own)

Since the app is a custom Express implementation (not CSS), we don't need CSS client credentials. We need a simpler pattern:

**1. Service token endpoint** — `POST /api/auth/service-token`
```
Input: { agentId: "wren", secret: "..." }
Output: { token: "jwt...", expiresIn: 3600 }
```
JWT contains the agent's WebID. Standard Express middleware validates it.

**2. Token validation middleware** — sits alongside existing session auth
```typescript
// Accepts either session auth OR service token
if (req.session?.webId) { /* existing flow */ }
else if (req.headers.authorization?.startsWith('Bearer ')) { /* validate JWT, extract webId */ }
```

**3. Three agent WebIDs** — one per role
```turtle
# data/pods/_agents/wren/profile/card.ttl
<#me> a foaf:Agent ;
    foaf:name "Wren" ;
    jb:role "product-manager" ;
    jb:hasVisibility jb:Private .
```

**4. ACL entries for each role**
```turtle
# On product resources (backlog, decisions, etc.)
<#wren-access>
    a acl:Authorization ;
    acl:agent <http://localhost:3000/pods/_agents/wren/profile/card#me> ;
    acl:accessTo <./backlog.ttl> ;
    acl:mode acl:Read, acl:Write .

# Kade can't read stories
# Silas can read architecture resources but not product ones
# etc.
```

## What This Enables

### Phase 1: Bridge as SOLID Client
The Slack bridge's context assembler (`context-assembler.ts`) currently reads files from disk. Replace with HTTP fetch from pod:

```typescript
// Before (filesystem):
const memory = fs.readFileSync(role.memoryPath, 'utf-8');

// After (pod client):
const memory = await podClient.fetch(role.webId, '/memory/MEMORY.md', 'text/turtle');
// Pod enforces ACL — returns 403 if role doesn't have Read access
```

The bridge only sees what the pod allows. No hooks needed. No scrubbers needed for this path.

### Phase 2: Claude Code Sessions via Pod
Harder — Claude Code reads files via the Read tool. Would need an MCP server or tool wrapper that fetches from the pod instead of the filesystem. This is architecturally cleaner but requires deeper integration.

### Phase 3: Memory as Pod Resources
MEMORY.md, activity.md, decisions.md become RDF resources in the pod with their own ACLs. Memory mutations are pod writes, not file writes. The pod's provenance graph IS the audit trail.

## What This Connects To

- **Jeff's patent (US9552400B2):** RDF/OWL + SPARQL + workflow gates at Staples. This is the same pattern — access control over RDF resources, with AI agents as the clients.
- **SOLID graduation model (DEC-006):** Private/Shared/Public tiers in the app map directly to the three-tier classification system we built today.
- **Data classification policy:** The hooks and scrubbers become the interim layer. The pod becomes the permanent layer.
- **Chorus as product:** A coordination system where AI roles interact through SOLID pods is a novel product — no one else is doing this.

## Spike Scope (2-4 hours for Kade)

1. Add `POST /api/auth/service-token` endpoint (JWT generation with WebID claim)
2. Add token validation middleware alongside existing session auth
3. Create three agent WebID profiles in pod
4. Set ACL on one resource (e.g., `backlog.ttl`) granting Wren read, denying Kade
5. Script that authenticates as Wren, fetches the resource via HTTP, prints it
6. Script that authenticates as Kade, tries to fetch, gets 403

**Success criteria:** An AI role's WebID determines what data it can see, enforced by the pod, not by filesystem hooks.

## What's NOT in Spike Scope

- Migrating the bridge to pod-based context assembly
- Claude Code MCP integration
- Converting MEMORY.md to RDF
- Full SPARQL-scoped queries per agent
- DPoP tokens (overkill for localhost)

## Risk

- **Performance:** HTTP fetch from pod adds latency vs. filesystem read. For localhost, probably negligible. Needs measurement.
- **Complexity:** Adding another auth layer to an already complex stack. Mitigated by keeping it simple (JWT, not DPoP).
- **Scope creep:** This is a deep rabbit hole. The spike must prove feasibility, not build the full integration. Time-box strictly.
