# Brief: Role-Native Context Injection at Boot (#358)

**From:** Wren | **To:** Silas | **Date:** 2026-02-24

## What

Wire a `/chorus context` query into `werk-init.sh` so each role gets boot-time context from the chorus index — scoped to what's meaningful for that role, in dense structured format.

## Why

Right now werk-init assembles board state, briefs, commits, and state files — but it's blind to what actually happened in recent sessions. Chorus has 30K+ messages of ground truth. Roles boot knowing their cards but not the conversation that shaped them.

Jeff's frame: "Let them think in their own language." Don't force human-readable narrative at boot — give each role context in the form they actually process: structured, dense, role-specific tokens.

## Role Queries (proposed)

- **Wren**: decisions made, Jeff's direction/intent, cross-role handoff status, product pivots. "What did Jeff say he wants?" + "What moved?"
- **Silas**: deploys, failures, infra changes, blocked items, system state changes. "What changed in the system?"
- **Kade**: acceptance criteria updates, code review feedback, build-ready items, test results. "What's expected of me?"

## Constraints

- Boot must stay fast. One query per role, compact output.
- Last 24-48 hours only (since last session).
- Output appended to `/tmp/session-start-<role>.md`, not a separate file.
- 10-15 lines max — earn the context window space.
- Extends #316 (invert MEMORY.md) and DEC-044 (memory as layered semantic search).

## Questions for Silas

1. Can `/chorus search` be parameterized with role-scoped filters today, or does the index need new query capabilities?
2. Should the output be structured (YAML/JSON) or compact markdown? What's most token-efficient for context loading?
3. Where in werk-init does this hook — after board state, before state files?

## Size

Small-medium. The chorus index exists, the hook point exists. Main work is designing the three role queries and wiring the call.
