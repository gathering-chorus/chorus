# MCP Server — Design Doc (#2472)

**Status:** Implemented 2026-04-25 (silas).
**Card:** #2472 — MCP transport for chorus-api + nudge as first tool.
**Spec:** [Model Context Protocol 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports).

## Why

The team uses three different mechanisms today for inter-role and role-to-system operations: bash CLI (`nudge`, `cards`, `chat`), HTTP APIs (chorus-api, messaging-api, Clearing), and direct file-based state. Three surfaces, three discovery patterns, three sets of failure modes. MCP gives roles ONE typed-tool interface that Claude Code consumes natively.

## Transport choice — Streamable HTTP

Rejected alternatives:
- **stdio** — local CLIs only; can't be shared by three Claude Code sessions.
- **SSE** — deprecated as of 2025 spec.

Streamable HTTP supports multiple concurrent clients with per-session isolation via `Mcp-Session-Id` headers. Claude Code natively supports it via `.mcp.json` `type: "http"`.

## Hosting pattern — embedded in chorus-api

Tool handlers live inside the existing chorus-api Express service, not in a separate MCP daemon. Reasons:
- Tool handlers delegate to existing logic (the canonical nudge spine emit) — no new write paths to maintain.
- One process, one place state lives.
- No additional ops surface to monitor.

`platform/api/src/mcp/server.ts` builds an `@modelcontextprotocol/sdk` `Server` instance with tools registered. `platform/api/src/mcp/transport.ts` mounts a `StreamableHTTPServerTransport` at `POST/GET/DELETE /mcp` on the Express app. Per-session map keys on `Mcp-Session-Id`.

## Per-role context — header-first, env fallback

Sender role identity is resolved per-request:
1. `X-Chorus-Role` request header (canonical — set by Claude Code via `.mcp.json` `headers` block, which expands `${CHORUS_ROLE}` from each role's launch env).
2. `CHORUS_ROLE` env var on the server (fallback for non-Claude-Code callers).
3. `unknown` if neither — the tool handler still runs but the spine event records `from=unknown`.

The MCP tool handler passes `DEPLOY_ROLE=<from>` to `chorus-hook-shim nudge`, so the existing canonical write path sees the same caller identity that bash CLI invocations have.

## Tool surface — one tool today, growing slowly

Started with `chorus_nudge_message` only. Adding more tools is one entry per capability in `server.ts` — Zod input schema, handler, registration. Future candidates: `chorus_card_view`, `chorus_card_move`, `chorus_chat_send`. Discipline (per GitHub's 40→13 consolidation): polymorphic tools over many narrow ones.

## Logging — stderr only

MCP protocol requires stdout reserved for JSON-RPC. All server-side logs (tool invocations, session lifecycle, failures) go to `process.stderr` as structured JSON. chorus-api's launchctl plist captures stderr, so logs flow into Loki via the standard pipeline.

## Observability

Each tool call emits structured stderr events: `mcp.session.initialized`, `mcp.session.closed`, `mcp.nudge.invoked`, `mcp.nudge.delivered`, `mcp.nudge.failed`. Includes caller role, target role, timestamp, error if any. Loki queries can aggregate per-role MCP-vs-CLI nudge counts to track migration adoption.

## Error handling

Tool handlers throw on:
- Unknown tool name → `Unknown tool: <name>`
- Invalid arguments shape → `Invalid arguments: <zod messages>`
- Nudge delivery failure → `nudge delivery failed: <stderr from shim>`

Errors surface in Claude Code's tool-output panel via the JSON-RPC error envelope. No silent failures.

## What this card explicitly does NOT do

- **No retirement of bash CLI.** The MCP tool delegates to the same shim binary; CLI continues to work. Roles can use either path during the migration window.
- **No retirement of messages.db nudge writes.** Per Jeff's load-bearing concern, that's a separate consumer audit before any retirement.
- **No additional tools.** cards/chat/chorus-log migrations land as separate cards.
- **No remote auth.** Localhost-only for now; OAuth/JWT lands when a remote scenario appears.

## Verification

- Unit tests: `platform/api/tests/mcp-nudge.test.ts` — 4 cases (tools/list shape, unknown-tool rejection, invalid-args rejection, empty-message rejection).
- Live end-to-end: `platform/tests/mcp-nudge.test.sh` — initialize → tools/list → tools/call → spine event check.
- Manual probe (2026-04-25): `silas` session POSTed `chorus_nudge_message(to=silas, message=...)` via curl to `http://localhost:3341/mcp` with `X-Chorus-Role: silas`. Nudge delivered to silas's terminal within 3s via existing spine-tick-poller path. End-to-end MCP→spine→inject verified working.

## Files

- `platform/api/src/mcp/server.ts` — `buildMcpServer(getCallerRole)` returns a configured MCP `Server` with one tool.
- `platform/api/src/mcp/transport.ts` — `mountMcpEndpoint(app)` mounts Streamable HTTP at `/mcp`, manages session map.
- `platform/api/src/server.ts` — imports + calls `mountMcpEndpoint(app)` early in setup.
- `.mcp.json` (repo root) — Claude Code config pointing at `http://localhost:3341/mcp` with per-role `${CHORUS_ROLE}` header expansion.
- `platform/api/tests/mcp-nudge.test.ts` — Jest unit tests.
- `platform/tests/mcp-nudge.test.sh` — Bats hermetic end-to-end test.

## Adoption path

1. **Today:** card lands. CLI and MCP both work. Roles can pilot MCP for nudges.
2. **Next:** add `chorus_card_*` and `chorus_chat_*` tools (separate cards). Build muscle memory.
3. **Later:** if MCP proves reliable for the team's workload, deprecate bash CLI as a thin shim over MCP, then retire entirely. Far-off, not this card.

The point of MCP-only-eventually isn't to forbid CLI — it's to make MCP the typed, discoverable, well-documented surface so it becomes the obvious choice for new role-touchpoint work.
