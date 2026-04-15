# ADR-014: Pod-Mediated Coordination

**Status:** Deferred — accepted but not shipped. Briefs remain filesystem-based. Chorus index (SQLite + LanceDB) serves the coordination role pods were intended for
**Date:** 2026-02-20
**Card:** #82
**Depends on:** SOLID-Mediated AI Access spike (5e3c843), ADR-002 (ACL Graduation)

## Context

Team coordination currently flows through the filesystem: briefs as markdown files in role directories, state files (next-session.md, current-work.md) as plain text, shared logs (decisions.md, activity.md) as append-only markdown. This works but is fragile, unauditable, and can't support autonomous role activation.

The SOLID spike proved AI roles can authenticate via service tokens and access pods through ACL-controlled endpoints. Card #82 wires that mechanism into actual coordination.

## Decision

Migrate coordination artifacts to SOLID pods in three phases, with mandatory filesystem fallback at every step.

### Phase 1: Briefs (this ADR)

Briefs move to pods first. They're the simplest artifact — write-once, read-many, clear ownership, already have notification flow.

**Pod structure:**
```
/pods/jeff/coordination/
  briefs/
    to-architect/     ← Wren+Kade write, Silas read
    to-engineer/      ← Wren+Silas write, Kade read
    to-pm/            ← Silas+Kade write, Wren read
```

**Format:** Markdown files stored as binary pod resources (not RDF). Filename convention unchanged: `YYYY-MM-DD-<topic>.md`.

**ACL model:**
- Container-level ACLs on each `to-<role>/` directory
- Sender roles get `acl:Write` + `acl:Append`
- Recipient role gets `acl:Read`
- Jeff's WebID gets `acl:Control` on all
- ACLs pre-configured at container creation — no manual grants needed

**Write flow:**
1. Bridge (or role) authenticates via `POST /api/auth/service-token`
2. Writes brief via `PUT /api/service/pods/jeff/coordination/briefs/to-architect/2026-02-20-topic.md`
3. On failure → fall back to filesystem write at `architect/briefs/2026-02-20-topic.md`
4. Log fallback event to Loki

**Read flow:**
1. Role authenticates via service token
2. Lists container: `GET /api/service/pods/jeff/coordination/briefs/to-architect/`
3. Reads specific brief: `GET /api/service/pods/jeff/coordination/briefs/to-architect/2026-02-20-topic.md`
4. On failure → fall back to filesystem read at `architect/briefs/`
5. Log fallback event

**Notification:** Bridge's BriefWatcher adds pod container polling alongside filesystem scanning. Either source triggers the Slack notification.

### Phase 2: Role State (future ADR)

`next-session.md` and `current-work.md` move to `/pods/jeff/coordination/state/<role>/`. Same fallback pattern.

### Phase 3: Shared Logs (future ADR)

`decisions.md` and `activity.md` move to `/pods/jeff/coordination/shared/`. Append-only ACL for all roles.

## Constraints

1. **Filesystem fallback is mandatory.** Every pod read/write falls back to filesystem. Zero hard cutovers.
2. **Pre-configured permissions.** Agent WebID creation auto-grants coordination ACLs. Jeff never manually grants.
3. **Closed artifact list.** Only: briefs, role state, decisions log, activity log. Adding types requires a new DEC.
4. **No code workflow disruption.** Git, TypeScript, Docker, Terraform stay filesystem. Pods are coordination only.
5. **Graceful degradation test.** Before shipping: stop Fuseki, run full session cycle. Everything works via fallback.

## Consequences

**Positive:**
- ACL-enforced access control on coordination (not just filesystem permissions)
- Audit trail on every read/write (AuditService logs with agent identity)
- Foundation for autonomous role activation (roles can poll their pod for new briefs)
- SPARQL-queryable coordination state (future, when/if we add RDF metadata)

**Negative:**
- HTTP overhead vs filesystem reads (mitigated by local network)
- New failure mode: pod unavailable (mitigated by mandatory fallback)
- Bridge needs service token management (token refresh, error handling)

**Neutral:**
- Markdown stays markdown — no RDF conversion tax for Phase 1
- Filesystem remains the canonical backup for all phases
