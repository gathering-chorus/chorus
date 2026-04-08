# Brief: Clearing as standalone always-on service

**From:** Wren
**Card:** #265 (Clearing voice tuning)
**Priority:** P1 — Jeff directed this morning

## Context

The clearing currently runs as an ad hoc foreground process — blocks a terminal, requires manual launch. Jeff wants it always available without blocking any terminal or requiring a new tab to start it.

## Decision

Make the Clearing its own standalone service:
- Fixed port 3460
- Always running via LaunchAgent (or Docker — your call on hosting strategy)
- The existing codebase at `chorus/clearing/` already has its own package.json, Express server, Socket.io — it's structurally an app already

## What Wren is doing (product/skill side)

- Chorus context injection: board state + recent decisions + team activity injected at session creation
- Already wired `--chorus` flag in the launcher (pending the service refactor)
- Will update the `/clearing` skill to POST context to the running service and open the browser

## What Silas needs to decide

1. **LaunchAgent vs Docker container** — LaunchAgent is simpler (no network bridge needed for Chorus API access). Docker keeps it in the compose stack. Your call.
2. **Port 3460** — available? Conflicts?
3. **Session management** — currently server = session. For always-on, we need a sessions concept. POST /api/sessions creates one, GET / shows the active one. Multiple concurrent sessions would be a bonus but not required for V1.

## What's preserved

- Nudge bridge (bidirectional terminal ↔ clearing)
- Transcript indexing to Chorus
- Decision auto-capture
- Grounding rules

## Existing code

- `chorus/clearing/bin/clearing` — launcher
- `chorus/clearing/src/server.ts` — Express + Socket.io server
- `chorus/clearing/src/participants.ts` — role definitions, context injection already wired
- `chorus/clearing/src/transcript.ts` — message handling
