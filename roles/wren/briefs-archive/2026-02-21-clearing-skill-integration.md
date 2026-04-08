# Brief: Clearing Skill Integration

**From**: Silas (Architect) → Wren (PM)
**Date**: 2026-02-21
**Card**: #107 (Chorus board)
**Context**: Jeff wants the Clearing available to all roles as part of your interaction normalization work.

## What Exists

The Clearing (C#16) is shipped as a CLI launcher at `chorus/clearing/bin/clearing`. Express + Socket.IO + Anthropic SDK. Multi-party chat: Jeff + 3 AI roles in a browser window. Decisions auto-captured. Transcripts indexed into Chorus. First session: 63 msgs, $0.12 Haiku.

## What's Needed

Make the Clearing invocable from any role's Claude Code session — including yours. The goal: **any role can start a Clearing session when multi-party alignment is needed**, not just Jeff from the terminal.

### Technical Shape

1. **Register `/clearing` as a skill** (like `/chorus` today)
   - Skill lives at `~/.claude/skills/clearing/`
   - Any role can invoke it: `/clearing` or `/clearing --topic "topic"`
   - Skill starts the server, opens the browser, Jeff chats, server returns structured context (decisions, action items) back to the calling session

2. **On-demand, not persistent** — server starts when invoked, stops when chat ends. No new infrastructure to manage.

3. **Structured return** — the Clearing already captures decisions and transcripts. The skill wrapper needs to pass those back to the calling role's context so decisions don't get lost between sessions.

4. **Topic/card linking** — optional `--card N` flag ties the session to a kanban card for traceability.

## What I Need From You

- **Product framing**: How does this fit into the interaction normalization work you're doing with Jeff? Is `/clearing` the right invocation point, or does it need to be more integrated into the session flow?
- **Role initiation rules**: Can any role start a Clearing? Or should it be moderated (e.g., Wren initiates, others join)?
- **Priority call**: Where does this land relative to your current work with Jeff on Chorus interactions?

## Constraints

- Runs on Haiku (~$0.10/session) — cheapest multi-role option
- Server is Node/TypeScript, already built and tested
- Skill registration follows the same pattern as `/chorus`

## My Recommendation

Ship it as an on-demand skill first. Keep it simple — `/clearing` starts a chat, decisions come back. Refinements (persistent service, auto-invocation, deeper session integration) can follow based on how Jeff and you use it.
