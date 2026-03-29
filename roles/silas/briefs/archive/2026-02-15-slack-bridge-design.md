# Slack-to-Claude Bridge — Architectural Design

**From**: Silas (Architect)
**Date**: 2026-02-15
**Priority**: P1
**Status**: Design — pending Wren input + Kade build

---

## Problem

Claude Code sessions are request-response. Roles only exist when Jeff is at the keyboard sending prompts. If Jeff messages #silas from his phone, nothing happens until he walks back to the Mac and starts a session. This defeats the purpose of async communication.

**Goal**: When someone posts a message addressing a role in Slack, that role responds — within 60 seconds, without a Claude Code session running.

---

## Architecture

### Overview

A lightweight Node.js service running in Docker that:
1. Polls Slack channels for new messages (every 30 seconds)
2. Determines which role is being addressed
3. Assembles role context from files on disk
4. Calls Claude API with context + message
5. Posts the response back to Slack

```
┌─────────────────────────────────────────────────┐
│                 Slack Workspace                  │
│  #silas  #wren  #kade  #all-gathering           │
└──────┬──────────────────────────┬───────────────┘
       │ poll (30s)               ▲ post response
       ▼                          │
┌──────────────────────────────────────────────────┐
│              slack-bridge (Docker)                │
│                                                  │
│  ┌───────────┐  ┌────────┐  ┌────────────────┐  │
│  │  Channel   │→│ Router │→│    Context      │  │
│  │  Monitor   │  │        │  │    Assembler   │  │
│  └───────────┘  └────────┘  └───────┬────────┘  │
│                                      │           │
│  ┌───────────┐  ┌────────┐  ┌───────▼────────┐  │
│  │   Rate    │←│Response│←│  Claude API    │  │
│  │  Limiter   │  │ Poster │  │  (Sonnet)     │  │
│  └───────────┘  └────────┘  └────────────────┘  │
│                                                  │
│  ┌────────────────────────────────────────────┐  │
│  │  /metrics (Prometheus)  /health            │  │
│  └────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────┘
       │ read-only mounts
       ▼
┌──────────────────────────────────────────────────┐
│  Team Directories (host filesystem)              │
│  architect/  product-manager/  engineer/         │
│  messages/   meetings/                           │
└──────────────────────────────────────────────────┘
```

### Why Polling (Not Socket Mode)

- **No Slack app reconfiguration.** Current bot token works as-is. Socket Mode requires enabling it in Slack admin + generating an App-Level Token (xapp-...).
- **Simpler to debug.** One HTTP call on a timer vs. a persistent WebSocket connection.
- **30-second latency is fine.** Jeff said 2-minute response window is acceptable. 30 seconds is well within that.
- **Upgrade path exists.** Can switch to Socket Mode later for real-time if needed. The internal architecture doesn't change — only the Channel Monitor component swaps.

---

## Components

### 1. Channel Monitor
- Polls `conversations.history` for each configured channel every 30 seconds
- Tracks `last_seen_ts` per channel (persisted to a small JSON file for restart resilience)
- Filters out bot's own messages (prevents response loops)
- Filters out messages older than last poll

### 2. Message Router
Determines which role responds to a message:

| Message location | Addressing | Who responds |
|---|---|---|
| #silas | Any message | Silas |
| #wren | Any message | Wren |
| #kade | Any message | Kade |
| #all-gathering | Contains "silas" (case-insensitive) | Silas |
| #all-gathering | Contains "wren" (case-insensitive) | Wren |
| #all-gathering | Contains "kade" (case-insensitive) | Kade |
| #all-gathering | Addresses multiple roles | Each addressed role responds |
| #all-gathering | No role named | No response |
| Any channel | Message from bot | Ignored (loop prevention) |

### 3. Context Assembler
Builds the system prompt for each role from files on disk:

| Source | Purpose | Read strategy |
|---|---|---|
| Role CLAUDE.md | Role identity + instructions | Full file |
| `team-architecture.md` | Team operating model | Full file |
| Role memory (MEMORY.md) | Persistent cross-session knowledge | Full file |
| Channel history (last 10 msgs) | Conversational continuity | From Slack API |
| `briefs/` inbox | Awareness of pending work | File listing only (names + dates) |
| `activity.md` (last 30 lines) | Recent team activity | Tail of file |

**Estimated context size**: ~4,000-6,000 tokens per request.

The system prompt includes a preamble:
```
You are responding via Slack, not in a Claude Code session.
You CAN: answer questions, discuss, read files, acknowledge, summarize.
You CANNOT: write files, run commands, make commits, or change system state.
If a request requires those capabilities, say: "I'll need a Claude Code session for that — can you start one?"
Keep responses concise — this is chat, not a brief.
```

### 4. Claude Client
- Uses **Claude Sonnet** (fast, cost-effective for chat — Opus reserved for Claude Code sessions where depth matters)
- Anthropic SDK (`@anthropic-ai/sdk`)
- Max response tokens: 1,024 (keeps responses chat-appropriate)
- Temperature: 0.7 (slightly conversational)
- Tool use (Phase 1): **read_file** only — allows reading specific files when asked

### 5. Response Poster
- Posts Claude's response to the same channel
- Prefixes with role indicator: `**Silas**: <response>` (so it's clear who's speaking)
- Threads replies when responding to a thread (uses `thread_ts`)
- Splits long responses (>3,000 chars) into multiple messages

### 6. Rate Limiter
- **Per role**: Max 15 API calls per hour (configurable)
- **Per channel**: Min 10 seconds between responses (debounce rapid messages)
- **Global**: Max 30 API calls per hour total
- **Monthly alert**: Log warning when projected monthly cost exceeds threshold (default $50)
- When rate limited: posts "I've hit my rate limit for this hour. I'll be back shortly." (once, not per message)

### 7. Health & Metrics
- `GET /health` — returns 200 if service is running and Slack API is reachable
- `GET /metrics` — Prometheus format:
  - `slack_bridge_messages_received_total{channel, role}`
  - `slack_bridge_responses_sent_total{channel, role}`
  - `slack_bridge_api_calls_total{role}`
  - `slack_bridge_api_latency_seconds{role}`
  - `slack_bridge_rate_limited_total{role}`
  - `slack_bridge_errors_total{type}`

---

## File Structure

```
messages/slack-bridge/
├── docker-compose.yml
├── Dockerfile
├── package.json
├── tsconfig.json
├── .env.example
├── src/
│   ├── index.ts              # Entry point, poll loop orchestrator
│   ├── config.ts             # Configuration loading + validation
│   ├── channel-monitor.ts    # Polls Slack, tracks last_seen_ts
│   ├── router.ts             # Message → role routing logic
│   ├── context-assembler.ts  # Reads files, builds system prompt
│   ├── claude-client.ts      # Anthropic API wrapper
│   ├── response-poster.ts    # Posts responses back to Slack
│   ├── rate-limiter.ts       # Per-role, per-channel rate limiting
│   └── metrics.ts            # Prometheus metrics server
├── config/
│   └── roles.json            # Role definitions
├── data/
│   └── .gitkeep              # Runtime state (last_seen_ts) — gitignored
└── tests/
    ├── router.test.ts
    ├── context-assembler.test.ts
    ├── rate-limiter.test.ts
    └── channel-monitor.test.ts
```

### roles.json
```json
{
  "roles": [
    {
      "name": "silas",
      "channel": "silas",
      "claudeMdPath": "/team/architect/CLAUDE.md",
      "memoryPath": "/memory/silas/MEMORY.md",
      "briefsPath": "/team/architect/briefs",
      "maxCallsPerHour": 15
    },
    {
      "name": "wren",
      "channel": "wren",
      "claudeMdPath": "/team/product-manager/CLAUDE.md",
      "memoryPath": "/memory/wren/MEMORY.md",
      "briefsPath": "/team/product-manager/briefs",
      "maxCallsPerHour": 15
    },
    {
      "name": "kade",
      "channel": "kade",
      "claudeMdPath": "/team/engineer/CLAUDE.md",
      "memoryPath": "/memory/kade/MEMORY.md",
      "briefsPath": "/team/engineer/briefs",
      "maxCallsPerHour": 15
    }
  ],
  "sharedChannel": "all-gathering",
  "pollIntervalMs": 30000,
  "globalMaxCallsPerHour": 30
}
```

### docker-compose.yml
```yaml
version: '3.8'

services:
  slack-bridge:
    build: .
    container_name: slack-bridge
    restart: unless-stopped
    ports:
      - "3460:3460"          # Metrics/health
    env_file:
      - ../../.env.bridge     # SLACK_BOT_TOKEN + ANTHROPIC_API_KEY
    volumes:
      # Team directories — READ ONLY
      - ../../architect:/team/architect:ro
      - ../../product-manager:/team/product-manager:ro
      - ../../engineer:/team/engineer:ro
      - ../../messages:/team/messages:ro
      - ../../meetings:/team/meetings:ro
      # Role memory directories — READ ONLY
      - ~/.claude/projects/-Users-jeffbridwell-CascadeProjects-architect/memory:/memory/silas:ro
      - ~/.claude/projects/-Users-jeffbridwell-CascadeProjects-product-manager/memory:/memory/wren:ro
      - ~/.claude/projects/-Users-jeffbridwell-CascadeProjects-engineer/memory:/memory:/memory/kade:ro
      # Runtime state (last_seen timestamps)
      - ./data:/app/data
    networks:
      - default
      - observability
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3460/health"]
      interval: 30s
      timeout: 5s
      retries: 3

networks:
  observability:
    external: true
    name: observability-network
```

### .env.bridge (NOT committed — in .gitignore)
```
SLACK_BOT_TOKEN=xoxb-...
ANTHROPIC_API_KEY=sk-ant-...
BRIDGE_PORT=3460
LOG_LEVEL=info
```

---

## Security

| Concern | Mitigation |
|---|---|
| API key exposure | Keys in .env.bridge, never committed. Docker env_file injection. |
| File system access | All mounts are `:ro` (read-only). Bridge cannot modify team files. |
| Response loops | Bot's own user ID filtered out. Messages from bot ignored. |
| Cost runaway | Per-role + global rate limits. Monthly cost alert threshold. |
| Prompt injection | Bridge preamble constrains behavior. No shell access. No write tools. |
| Slack token scope | Uses existing bot token. No additional OAuth scopes needed for read + post. |

---

## Observability

- **Prometheus**: scrape `slack-bridge:3460/metrics` (add to prometheus.yml)
- **Alerts**: `SlackBridgeDown` (health check fails), `SlackBridgeHighErrorRate` (>5 errors/hour)
- **Logs**: Structured JSON to stdout → Promtail picks up from Docker → Loki
- **Grafana**: Add panel to team dashboard showing bridge activity (messages/responses/errors per role)

---

## What Changes for the Team

| Before | After |
|---|---|
| Roles only exist in Claude Code sessions | Roles have persistent Slack presence |
| Jeff must be at keyboard to get a response | Jeff can message from phone, get response in ~30s |
| Roles can't talk to each other between sessions | Roles can exchange via Slack asynchronously |
| Slack is signal-only (ephemeral) | Slack becomes light conversational channel |

**What doesn't change:**
- Claude Code sessions are still where real work happens (writing, building, committing)
- Briefs are still the source of truth for substance
- The bridge is chat, not execution

---

## Phases

| Phase | Scope | Dependency |
|---|---|---|
| **1 (this build)** | Polling + read-only file access + chat | None — bot token + API key exist |
| **2** | Socket Mode (real-time) | Slack app admin: enable Socket Mode, generate xapp token |
| **3** | Tool use expansion (read specific files on demand) | Phase 1 stable |
| **4** | Supervised write (bridge drafts, Jeff approves via Slack reaction) | Phase 3 stable + approval UX design |

---

## Cost Estimate

| Item | Estimate |
|---|---|
| Model | Claude Sonnet (input: $3/MTok, output: $15/MTok) |
| Avg context per call | ~5,000 tokens |
| Avg response per call | ~500 tokens |
| Calls per day (typical) | ~50 (light async use) |
| Daily cost | ~$1.10 |
| Monthly cost | ~$33 |
| Rate-limited max/month | ~$90 (if all limits hit constantly) |

---

## Questions for Wren (Product)

1. **Scope**: All three roles from day one, or prove with one first? My recommendation: all three — the service is parameterized, not per-role.
2. **Personality calibration**: Should Slack responses feel different from Claude Code responses? Shorter? More casual? Or same voice, just constrained by capability?
3. **#all-gathering behavior**: When a message doesn't address a specific role, should anyone respond? Or strict silence? I lean toward silence — avoid noise.
4. **Escalation language**: When the bridge can't do something (needs a Claude Code session), what's the right way to say that? Should it tag Jeff? Just state the limitation?

## Questions for Kade (Build)

1. **Estimated build time**: Given the component breakdown, what's your read?
2. **TypeScript or plain JS?**: The app codebase is TypeScript. I'd say match it for consistency, but your call on what's fastest.
3. **Test strategy**: I've outlined unit tests for router, context-assembler, rate-limiter. Integration test: mock Slack API + mock Claude API, verify end-to-end flow. Anything else you'd add?
4. **Deployment**: Docker build + add to shared-observability prometheus.yml. Any concerns with the volume mount approach?

---

*This is infrastructure that changes how the team operates. It inverts the communication model from pull (roles check when prompted) to push (roles respond when addressed). The Prometheus guardrail applies: this must be simple enough for Jeff to understand and control.*

— Silas, 2026-02-15
