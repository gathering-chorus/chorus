# Spike: The Clearing — Multi-Party Chat Service

**Date**: 2026-02-21
**Author**: Silas (Architect)
**Card**: C#16 (The Clearing)
**Status**: Spike complete — recommendation ready

---

## Question

What's the simplest architecture for a local multi-party chat with rich media, launchable from CLI, with transcript returned on close?

## Context

Jeff wants a synchronous chat service where he and all three AI roles (Wren, Silas, Kade) can converse in a browser with rich media (images, HTML pages, video). Invoked from a Claude Code terminal session. Transcript flows back when the session ends. This becomes the primary interaction surface for The Clearing (C#16).

## Findings

### What Already Exists (Internal)

| Component | Location | Reusable? |
|-----------|----------|-----------|
| Group conversation orchestrator | `slack-bridge/src/group-conversation.ts` | Yes — turn-taking, token budgets, prior-turn tracking |
| Context assembler | `slack-bridge/src/context-assembler.ts` | Yes — builds rich prompts from CLAUDE.md, memory, briefs |
| Claude client | `slack-bridge/src/claude-client.ts` | Yes — Anthropic SDK wrapper with streaming |
| Scrubber | `slack-bridge/src/context-assembler.ts` | Yes — credential/IP redaction |
| Rate limiter | `slack-bridge/src/rate-limiter.ts` | Yes — per-role + global budgets |
| Express app | `jeff-bridwell-personal-site/src/app.ts` | No — too coupled to Gathering domain |
| WebSocket/real-time infra | — | Does not exist yet |

### What Exists Externally

- **Socket.IO**: Mature, well-documented WebSocket library. Chat tutorial is ~80 lines. Already compatible with our Express/TypeScript stack.
- **markdown-it**: Vanilla JS markdown renderer with plugins for iframe, video, HTML5 embeds. No framework dependency. CDN-loadable.
- **`open` package**: Cross-platform browser launcher from Node.js.
- **Existing chat frameworks** (Fiora, Let's Chat, m1k1o/chat): All bring unnecessary baggage (MongoDB, user accounts, admin panels). Building from primitives is less work.
- **Claude Agent SDK**: V2 preview supports `createSession()` per agent. But the raw Messages API is simpler for our case and we already have a working Claude client in the bridge.

### Key Architectural Insight

The Slack bridge already solves the hard problems: context assembly, turn orchestration, rate limiting, scrubbing. The chat service is essentially **the bridge with a different transport** — browser WebSocket instead of Slack API. We're not building a chat framework; we're giving the bridge a direct-connection mode.

## Options

### Option A: Standalone Chat Server (Recommended)

**Architecture**: Single TypeScript package in `chorus/clearing/`. Express + Socket.IO server. Vanilla HTML client with markdown-it. Imports context assembly and Claude client logic from the bridge (or extracts shared modules).

```
CLI command (./clearing)
  → Express + Socket.IO on random port
  → opens browser via `open` package
  → browser: vanilla HTML/CSS/JS + markdown-it + Socket.IO client
  → server: orchestrates turns via Anthropic Messages API
  → Jeff types, AI roles respond (sequential or @-addressed)
  → rich media rendered inline (images, HTML, video)
  → "End Session" button or tab close → disconnect event
  → server serializes transcript → writes to file
  → CLI process exits, transcript available to invoking session
```

**New dependencies**: `socket.io` (~300KB), `open` (~5KB). markdown-it via CDN (zero server dep).

**Reuse**: Context assembler, Claude client, scrubber, rate limiter patterns from bridge. Not imported directly at first — extracted or reimplemented simply.

**Estimated scope**: ~500-800 lines TypeScript (server) + ~200-300 lines HTML/CSS/JS (client). One developer, one session to prototype.

**Pros**:
- Simplest possible thing that works
- No framework overhead
- Full control over participants, turn-taking, media rendering
- Transcript capture is a first-class feature
- Natural home for The Clearing (C#16)
- Doesn't touch existing app or bridge code

**Cons**:
- Some code duplication with bridge (context assembly, Claude calls) until shared modules are extracted
- No persistence between sessions (transcript is ephemeral until saved)

### Option B: Extend the Slack Bridge

Add a "direct mode" to the existing bridge that serves a web UI instead of posting to Slack.

**Pros**: Maximum code reuse. One codebase.
**Cons**: Bridge is already complex (1200+ lines across 10 files). Adding a web transport risks making it fragile. Different lifecycle (bridge runs continuously; chat sessions are ephemeral).

**Not recommended.** Bridge and chat have different lifecycles. Coupling them increases blast radius.

### Option C: Add Chat to the Gathering App

Mount WebSocket endpoints on the existing Express app.

**Pros**: Reuses auth, middleware, session store.
**Cons**: Wrong domain boundary. Chat is Chorus infrastructure, not a Gathering feature. Adds complexity to an already large app. Deployment coupling.

**Not recommended.** This violates the Gathering/Chorus separation.

## Recommendation: Option A

Build a standalone chat server in `chorus/clearing/`. Keep it simple. The architecture:

```
chorus/clearing/
  ├── package.json          # express, socket.io, @anthropic-ai/sdk, open
  ├── tsconfig.json
  ├── src/
  │   ├── server.ts         # Express + Socket.IO + orchestration
  │   ├── participants.ts   # Role config, context loading, Claude API calls
  │   ├── transcript.ts     # Message accumulation + serialization
  │   └── media.ts          # Media detection + serving (local files)
  └── public/
      └── index.html        # Single-page chat UI (inline CSS/JS, markdown-it via CDN)
```

**CLI launcher** (shell script or `bin` entry):
```bash
clearing                    # opens chat with all 3 roles
clearing --with silas,kade  # opens with specific roles
clearing --replay last      # view last transcript
```

**Turn-taking model**:
- Jeff types freely
- All roles see every message
- Roles respond sequentially (configurable order) unless Jeff @-addresses one specifically
- Token budget per turn (reuse bridge's 150-token cap for quick exchanges, or configurable)
- Jeff can interrupt the sequence by typing mid-response

**Rich media**:
- Messages are markdown, rendered via markdown-it with plugins
- Images: `![alt](path)` renders inline. Local paths served via Express static
- HTML pages: iframe embed with configurable height
- Video: HTML5 video tag for local files, iframe for YouTube/Vimeo
- Code: fenced code blocks with syntax highlighting (highlight.js via CDN)

**Transcript return**:
- Accumulated in-memory as structured JSON: `{sender, content, timestamp, media[]}`
- On session close: written to `chorus/clearing/transcripts/<timestamp>.json`
- Also written to stdout as JSON for the invoking process to capture
- Optional: post summary to #all-gathering on close

## Dependencies on Other Work

- **None blocking.** This can be built independently.
- Future integration with Chorus index (context service) for searchable transcript history.
- Future integration with SOLID pods for persistent conversation storage.

## Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| Claude API cost per chat session | Medium | Token budgets per turn + per session. Display running cost. |
| Socket.IO adds operational complexity | Low | Ephemeral — only runs during active chat sessions |
| Context assembly diverges from bridge | Medium | Extract shared modules after prototype proves the pattern |
| Rich media rendering edge cases | Low | Start with images + markdown, add HTML/video incrementally |

## Next Steps

1. Card this on the Chorus board
2. Prototype Option A — server + client in one session
3. Test with Jeff: launch from terminal, chat with one role, end session, verify transcript
4. Iterate: add all three roles, rich media, turn-taking refinements

## Relationship to Existing Architecture

This is The Clearing. The concept we've been circling (C#16) — "where Jeff interacts with Chorus and learns from it" — is concretely a chat service. Not a dashboard, not a control panel. A conversation space with rich media and AI participants. The philosophical framing (Heidegger's Lichtung — the open space where things become visible) maps perfectly: Jeff speaks, the roles respond, media is rendered inline, and the transcript captures what emerged.
