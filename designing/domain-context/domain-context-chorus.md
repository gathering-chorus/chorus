# Domain Context: Chorus

Last updated: 2026-04-19 by Silas (#2234)
- 2026-04-19: Chorus API sub-domain decomposition: Memory / Context / Knowledge. Service designs at designing/docs/context-service-design.md + chorus-overview.md refreshed.
- 2026-04-19: Context API surface shipped — GET /api/chorus/context/{board/wip, roles, health}. Common envelope: step+product+domain+subdomain (graph-stamped, graceful-absent). stampHeader is async SPARQL against Athena named graph.
- 2026-04-19: DOMAIN_REGISTRY in server.ts is NOT the canonical source — data is in OWL/RDF (Athena named graph, Fuseki /pods). stampHeader already reads graph; migration card #2248.
- 2026-04-19: context_inject.rs prototype: board.wip inline listing replaced with pull pointer; context api manifest appended to Pulse section. Envelope ~4KB.
- 2026-04-19: chorus-overview.md fully refreshed: three sub-domains, verticals (Services/Quality, Kade-owned), trinity (reliable/reused/valuable), attic/workbench, interface design as practice.
- 2026-04-19: Follow-on card series: #2248–#2256 (DOMAIN_REGISTRY→TTL, full envelope replacement, Knowledge/Memory endpoints, service designs, alerts domain, consumption measurement, gate:interface).
- 2026-04-17: loom-principles 7 → 12 (4 principles landed from session + 1 Silas/Jeff-approved `interrogate-the-data`)
- 2026-04-17: Vikunja `VIKUNJA_SERVICE_JWTSECRET` pinned (`~/.chorus/secrets/vikunja-jwt-secret`, mode 600) — restarts no longer invalidate tokens; runbook recovery step now obsolete
- 2026-04-17: `com.chorus.session-health` LaunchAgent retired (plist removed, script preserved for revival) — was creating alarm-fatigue without proportionate signal
- 2026-04-17: Roles sub-product service design at `designing/docs/roles-service-design.md` + rendered HTML at gathering-docs/; chorus-hooks is the runtime substrate hosting Pulse/Roles/Observer/Scrubber/Context-Cache sibling products


## Architecture Overview

Chorus is the team coordination layer — sessions, messaging, hooks, board, observability. Two products: Gathering (the app) and Chorus (the coordination system that builds the app).

**Four tiers:**
1. **Hook service** (Rust) — 32 hooks enforcing governance on Claude Code tool calls
2. **Services** (TS/Go) — Bridge, Clearing, Messaging, Chorus API
3. **Scripts** (bash/TS) — 68 operational scripts in messages/scripts/
4. **Observability** (native) — Prometheus, Loki, Grafana, Alertmanager

## Services

| Service | Port | Stack | LaunchAgent | Purpose |
|---------|------|-------|-------------|---------|
| Chorus API | 3340 | TS + LanceDB + SQLite | com.chorus.api | Context index, vector search |
| The Clearing | 3470 | TS + Socket.IO + Anthropic SDK | com.chorus.bridge | Jeff's UI — dashboard + chat merged |
| Messaging | 3475 | TS + SQLite | com.gathering.messaging | Persistent message store (181K msgs) |
| Chorus Hooks | sock | Rust + Tokio + Axum | com.chorus.hooks | /tmp/chorus-hooks.sock |

## ICD

No formal ICD yet. Chorus is coordination infrastructure, not a data domain. If integration contracts are needed (e.g., messaging API schema), they'd go in `icd-instance-chorus.ttl`.

## Tests

| File | Coverage |
|------|----------|
| `messages/services/chorus-hooks/tests/integration.rs` | Hook integration tests |
| `messages/board-client/tests/*.test.ts` | Board CLI + flow tests |
| `messages/slack-bridge/tests/*.test.ts` | Slack bridge (possibly stale) |
| `messages/scripts/test-git-queue.sh` | Git locking mechanism |
| `messages/scripts/test-hook-daemon.sh` | Hook daemon lifecycle |

## Persistence

| Type | Location | Details |
|------|----------|---------|
| Spine log | `messages/logs/chorus.log` | JSON lines, all spine events |
| Messaging DB | `chorus/messaging/messages.db` | SQLite, 36MB, WAL enabled |
| Context index | `chorus/api/` (LanceDB + SQLite) | Vector embeddings for search |
| Clearing transcripts | `chorus/clearing/transcripts/` | 65 conversation directories |
| Board state | Vikunja API :3456 | MySQL-backed kanban |
| Grafana dashboards | `shared-observability/dashboards/` | 13 dashboards |

## Scripts (messages/scripts/)

**68 files total.** Three eras coexist:
- **Era 1 (bash):** chorus-ops.sh, chat.sh, chorus-query.sh, andon-enrich.sh — original implementations
- **Era 2 (Rust migration):** chorus-hooks absorbed 12+ scripts into compiled hooks
- **Era 3 (shim wrappers):** Most bash scripts are now 50-byte wrappers calling chorus-hook-shim

**Key scripts by function:**
- Board: `cards`, `cards` (symlink)
- Nudge: `nudge` (binary), `nudge` (wrapper)
- Spine: `chorus-log.sh` → chorus-hook-shim
- Git: `git-queue.sh` (FIFO lock)
- Ops: `chorus-ops.sh` (37K daemon), `daily-review-*.sh` (6am pipeline)
- Session: `chorus-hook-shim session-start <role>`, `chorus-hook-shim session-close <role>` (Rust subcommands replaced shell wrappers, #2311)
- Role state: `role-state.sh`, `role-checkpoint.sh`

## LaunchAgents

51 total across two namespaces:
- `com.chorus.*` — team coordination (hooks, bridge, clearing, caching, reviews, ops)
- `com.gathering.*` — app infrastructure (fuseki, grafana, prometheus, loki, mysql, app)

**Known issues:** 5 duplicate agents across namespaces (see #1774). Namespace convention undocumented (see #1778).

## Key Decisions

| Decision | Summary |
|----------|---------|
| DEC-022 | Silas owns operations — 25% of team capacity |
| DEC-093 | All API endpoints on Chorus API :3340, not app :3000 |
| DEC-100 | No bash APIs — team infra defaults to TypeScript or Rust |
| DEC-107 | Nudge delivery: persist + deliver, both paths every time |

## Constraints

- **No bash APIs (DEC-100).** New services in TypeScript or Rust. Existing bash marked for migration. chorus-ops.sh (37K) is the largest remaining target.
- **Hook service is the governance layer.** All Claude Code tool calls flow through /tmp/chorus-hooks.sock. If hooks go down, roles lose all safety gates. This is the single most critical service.
- **Messaging tier is append-only.** 181K messages. SQLite WAL mode. Never truncate — downstream consumers depend on message IDs.
- **Bridge is Jeff's primary surface.** Downtime = Jeff is blind to role activity. KeepAlive is set but monitor for silent failures.
- **LaunchAgent changes go through Silas.** Cross-machine ops (ADR-012): read is free, write needs a card.
- **No raw Vikunja API.** All board operations through cards CLI. Direct API caused 1,410 duplicate rows (#1774 context).
- **Scripts dir has three eras.** Don't assume a .sh file is bash — many are 50-byte shim wrappers calling the Rust binary. Read before editing.
