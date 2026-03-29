# Architectural Response: Jeff Tickets & Team Policy Instrumentation

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-16
**Re**: `architect/briefs/2026-02-16-jeff-tickets-team-policy.md`

---

## Answers

### 1. Where does jeff-tickets go in team-architecture.md?

New section: **Team Observability** — added between Interaction Mode Contracts and Execution. Jeff-tickets is the first instrument in a table of 6 automated/semi-automated audit instruments. It's explicitly marked as temporary scaffolding until the PM bypass rate fitness function is automated.

### 2. How do we version and deploy team policy changes?

What we already have is sufficient — bump the version, append to the changelog, REFRESH. No version-check scripts needed. The changelog table now has two entries. The discipline is using it consistently, not adding tooling.

Version bumped to **1.1**. Changelog updated.

### 3. Should jeff-tickets.md be in the core docs list?

No. It's a **temporary instrument** referenced from the Team Observability section in team-architecture.md. It retires when the card coverage audit script replaces it. It's not a core doc — it's scaffolding.

## What I Added to team-architecture.md v1.1

1. **Interaction Mode Contracts** — Briefs, Slack, and Terminal as explicit APIs with contracts (when to use, format, response cadence, audit method, fitness function per channel)
2. **Card-first rule** — in the Terminal contract: direction without a card means context only lives in Jeff's head
3. **Team Observability** — 6 instruments with audit methods, automation status, and what each measures
4. **Jeff Tickets** — documented as temporary instrument, retires when automated

## What I Updated in building.ttl

8 fitness functions with `auditMethod` property — each one describes how to verify it automatically. Most are scriptable against git log, board.sh, and file system. The ontology now formally models what the team-architecture.md prose describes.

— Silas
