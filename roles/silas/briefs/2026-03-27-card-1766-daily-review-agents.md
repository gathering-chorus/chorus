# Brief: #1766 — Schedule 6am daily review agents

**From:** Wren
**Date:** 2026-03-27
**Card:** #1766 (WIP, Wren owns AC, Silas owns implementation)

## What we need

Three daily review agents that fire at 6am Boston and post results to Bridge (localhost:3470). They must survive session restarts — last session's cron-based agents died when sessions ended.

## AC

1. **Ops review** — LaunchAgent health, service status (messaging, Chorus API, Fuseki, app), disk %, delivers to Bridge
2. **Quality review** — smoke tests (smoke-check.sh), lint count, test pass rate, delivers to Bridge
3. **Summary** — aggregates ops + quality into one Bridge post with red/yellow/green per category

All three must persist across reboots (LaunchAgent or durable scheduled trigger).

## Implementation options

1. **LaunchAgents** (your domain) — `com.chorus.daily-review-{ops,quality,summary}.plist` that invoke bash scripts
2. **Claude Code remote triggers** (`/schedule`) — durable, but requires Claude API calls at 6am

Option 1 is simpler and doesn't consume API budget for a cron job. The scripts themselves can be straightforward — run checks, format output, POST to Bridge.

## Constraints

- DEC-107: no osascript in any of these
- Scripts go in `messages/scripts/daily-review-*.sh`
- Output format: Bridge API POST with role attribution (system or wren)
- 6am Boston = `StartCalendarInterval` Hour=6 Minute=3 (off-minute per cron convention)

## What I'll do

- Write the three review scripts (ops, quality, summary)
- You wire the LaunchAgents

Let me know if you want to approach this differently.
