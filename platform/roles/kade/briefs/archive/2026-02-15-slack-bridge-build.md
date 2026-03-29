# Brief: Slack-to-Claude Bridge — Build Spec

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-15
**Priority**: P1
**Action needed**: Review design, answer 4 questions, build when ready

---

## Context

Jeff wants roles to respond on Slack without a Claude Code session running. This is a new Docker service that gives all three roles persistent Slack presence.

## Design

Full design at: `architect/briefs/2026-02-15-slack-bridge-design.md`

Read the full doc — it has the component breakdown, file structure, docker-compose, config format, and security model. Here's the build summary:

## What to Build

**Service**: `messages/slack-bridge/` — Node.js TypeScript, Docker container.

**Components** (7):
1. **Channel Monitor** — Polls `conversations.history` every 30s, tracks `last_seen_ts` per channel
2. **Router** — Maps messages to roles (channel-based + name mention in #all-gathering)
3. **Context Assembler** — Reads role files from mounted volumes, builds system prompt
4. **Claude Client** — Calls Anthropic API (Sonnet), 1024 max tokens, `read_file` tool only
5. **Response Poster** — Posts back to Slack, handles threading, message splitting
6. **Rate Limiter** — 15/role/hour, 30 global/hour, 10s debounce per channel
7. **Metrics** — Prometheus endpoint on port 3460, health check

**Dependencies**: `@anthropic-ai/sdk`, `@slack/web-api`, `prom-client`, `typescript`

**Docker**: Mounts team dirs read-only, joins observability-network, port 3460 for metrics.

**Tests**: Unit tests for router, context-assembler, rate-limiter, channel-monitor. Integration test with mocked Slack + Claude APIs.

## Your Questions

1. **Estimated build time**: Given the component breakdown, what's your read?
2. **TypeScript or plain JS?**: App codebase is TS. I'd say match it. Your call on speed.
3. **Test strategy**: Outlined unit + integration. Anything else you'd add?
4. **Deployment**: Docker build + prometheus.yml scrape target. Concerns with volume mounts?

## Key Constraints

- **Read-only.** Bridge cannot write files, run commands, or change state. All volume mounts are `:ro`.
- **Bot loop prevention.** Filter out messages from the bot's own user ID.
- **Rate limits are firm.** When hit, post one "rate limited" message, then silence until reset.
- **Sonnet, not Opus.** Keep costs reasonable for chat. Opus stays in Claude Code.

## Existing Infrastructure

- Bot token: `messages/.env` (`SLACK_BOT_TOKEN`)
- Anthropic key: `jeff-bridwell-personal-site/.env` (`ANTHROPIC_API_KEY`)
- Slack API patterns: see `messages/scripts/slack-read.sh` and `slack-post.sh` for channel resolution
- Docker network: `observability-network` (same as Vikunja, WebVOWL)

— Silas
