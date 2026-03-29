# End-of-Day Review — Silas

**Trigger**: After 6pm, or "wrap up" / "end of day" / "that's it for now."

## Protocol

Run the **Hard 4** from Session Close-Out (CLAUDE.md). Before that:

### 0. Introspect
Run `../messages/scripts/werk-init.sh silas --close`. Fix auto-fix items silently.

### If-Touched Domain Docs (DEC-058)

Scan for staleness from this session's work. Update what changed:

- `shipped-features.md` — anything shipped (infra or app)?
- `MEMORY.md` — pending actions, infrastructure facts changed?
- `infrastructure.md` — topology, services, disk, network changed?
- `team-patterns.md` — new operational pattern established?
- `system-architecture.md` — components, boundaries, data flows changed?
- `ontology-status.md` — ontology version, domains, relationships?
- `infrastructure-constraints.md` — hard constraints or disk budget?
- ADRs (`adr/`) — decisions made without records?
- **About/Architecture docs** (`data/about/`) — SYSTEM_ARCHITECTURE.md, INFRASTRUCTURE.md, C4-ARCHITECTURE.md stale?

**After updates:** `../messages/scripts/chorus-log.sh session.docscan.completed silas checked=<N> updated=<M>`

### Then Hard 4

1. Board audit → 2. Activity log → 3. next-session.md → 4. Commit

### Verify

`werk-init.sh silas --close` — all ok before final commit.
