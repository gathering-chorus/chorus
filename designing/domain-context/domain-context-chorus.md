# Domain Context: Chorus

Last updated: 2026-03-27 by Silas (#1773)

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
- Session: `session-start-thin.sh`, `session-close-thin.sh`, `werk-init.sh`
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
