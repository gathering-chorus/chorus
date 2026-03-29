# Pod-Mediated Coordination — Phase 1: Briefs

**From:** Silas (Architect)
**Date:** 2026-02-20
**Card:** #82
**ADR:** ADR-014
**Priority:** P1

## What

Wire the SOLID spike into the coordination layer. Phase 1: briefs flow through pods instead of (in addition to) filesystem.

## Pod Structure to Create

```
data/pods/jeff/coordination/
  briefs/
    to-architect/
    to-engineer/
    to-pm/
```

Each directory needs a `.acl` file. Templates below.

## ACL Files

**`to-architect/.acl`**
```turtle
@prefix acl: <http://www.w3.org/ns/auth/acl#> .

<#owner>
    a acl:Authorization ;
    acl:agent <https://jeffbridwell.solidcommunity.net/profile/card#me> ;
    acl:accessTo <./> ;
    acl:default <./> ;
    acl:mode acl:Read, acl:Write, acl:Control .

<#reader>
    a acl:Authorization ;
    acl:agent <http://localhost:3000/pods/jeff/_agents/silas/profile/card.ttl#me> ;
    acl:accessTo <./> ;
    acl:default <./> ;
    acl:mode acl:Read .

<#writers>
    a acl:Authorization ;
    acl:agent <http://localhost:3000/pods/jeff/_agents/wren/profile/card.ttl#me>,
              <http://localhost:3000/pods/jeff/_agents/kade/profile/card.ttl#me> ;
    acl:accessTo <./> ;
    acl:default <./> ;
    acl:mode acl:Write, acl:Append .
```

Same pattern for `to-engineer/` (Kade reads, Wren+Silas write) and `to-pm/` (Wren reads, Silas+Kade write).

## Data Contracts

### Write a Brief
```
PUT /api/service/pods/jeff/coordination/briefs/to-architect/2026-02-20-topic.md
Authorization: Bearer <wren-or-kade-token>
Content-Type: text/markdown

<markdown body>
```
**Response:** 201 Created (new) or 200 OK (overwrite)
**Error:** 403 if sender doesn't have Write ACL

### List Briefs
```
GET /api/service/pods/jeff/coordination/briefs/to-architect/
Authorization: Bearer <silas-token>
```
**Response:** 200 with container listing (filenames)
**Error:** 403 if requester doesn't have Read ACL

### Read a Brief
```
GET /api/service/pods/jeff/coordination/briefs/to-architect/2026-02-20-topic.md
Authorization: Bearer <silas-token>
```
**Response:** 200 with markdown body
**Error:** 403 if requester doesn't have Read ACL

## Fallback Pattern

Every pod operation wraps in try/catch with filesystem fallback:

```typescript
async function writeBrief(toRole: string, filename: string, content: string): Promise<void> {
  try {
    const token = await getServiceToken(fromRole);
    await fetch(`${BASE}/api/service/pods/jeff/coordination/briefs/to-${toRole}/${filename}`, {
      method: 'PUT',
      headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'text/markdown' },
      body: content
    });
  } catch (err) {
    // Fallback to filesystem
    const fsPath = path.join(ROLE_DIRS[toRole], 'briefs', filename);
    await fs.writeFile(fsPath, content);
    logger.warn({ event: 'pod_fallback', toRole, filename, error: err.message });
  }
}
```

## Implementation Order

1. Create pod directory structure + ACL files
2. Add `writeBrief()` / `readBrief()` / `listBriefs()` utility with fallback
3. Wire into bridge's `commitment-brief-writer.ts` (uses `writeBrief()`)
4. Wire into bridge's `BriefWatcher` (adds pod container polling)
5. Wire into chorus reconciliation (reads from pod + filesystem)
6. **Graceful degradation test:** Stop Fuseki, run session cycle, verify filesystem fallback works

## What NOT to Change

- Brief format stays markdown (no RDF conversion)
- Filename convention unchanged
- Slack notifications still come from bridge
- Filesystem directories remain (fallback target)
- Role CLAUDE.md files stay on filesystem
- Git workflow unchanged

## Acceptance

- Bridge writes commitment briefs to pod (with filesystem fallback)
- Role reads briefs from pod via authenticated API (with filesystem fallback)
- BriefWatcher detects briefs from both pod and filesystem
- Graceful degradation test passes (Fuseki down = everything still works)
