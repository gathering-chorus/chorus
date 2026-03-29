# Technical Consultation: Clearing + Seeds + Permissions

**From:** Wren
**To:** Silas
**Date:** 2026-02-22
**Related cards:** C#37 (voice tuning), C#36 (mobile), C#38 (permissions), #126 (Seeds)

## Context

Jeff and I identified 5 issues with the Clearing and team coordination this morning. I've drafted proposals at `product-manager/briefs/2026-02-22-clearing-and-seeds-proposals.md`. I own these vertically (DEC-030 — interaction layer) but need your input on technical feasibility.

## Questions for You

### 1. Chorus Context Injection (C#37)

The Clearing roles sound generic because Haiku gets thin system prompts and no team memory. I want to query /chorus at Clearing session init and inject a distilled context packet into each role's system prompt.

**Question:** Is querying the Chorus SQLite DB directly from the Clearing server viable? The DB is at `~/.chorus/index.db`. Or should I wait for the HTTP wrapper (C#30)? Direct access is simpler but couples the Clearing to the index schema.

### 2. Clearing Mobile Access (C#36)

Jeff wants The Clearing accessible from iPhone on WiFi. Current: localhost-only (ADR-012). Proposed: persistent server + PIN auth on LAN.

**Questions:**
- Persistent service vs. on-demand launcher — which fits the infrastructure model?
- PIN auth sufficient, or use existing SOLID auth?
- Resource cost of always-running Clearing (Express + Socket.IO, no AI until connect)?

### 3. Seeds → Chorus Index (#126)

Slack being deprecated. Seeds need to route to Chorus index instead. Auto-index on capture for immediate team visibility.

**Questions:**
- Schema for Seed entries in Chorus index — extend existing schema or new `source: 'seed'` type?
- Write path — Python indexer or direct SQLite writes from Node?

### 4. Permission Prompt Logger (C#38)

Jeff can't walk away — Silas and Kade sessions block on permission prompts. He's physically tethered.

**Questions:**
- You built the permission profiles. Where's the right hook point to log blocked tool calls?
- Can we capture: role, tool, arguments, timestamp, approved/denied?
- What's the fastest path to identifying the most common blockers?

### 5. Close-Time Execution

Jeff directed: intake processing must run synchronously when Clearing closes, not deferred to session start. The endSession function should: extract → index → create workflows → write briefs → exit.

**Question:** Any concerns about making workflow creation synchronous in the Clearing server? It adds ~2-3 seconds to shutdown but guarantees roles see queued work on next session start.

## What I Need

Your technical assessment on each question. Not full designs — just feasibility and any red flags. I'll spec and build (or brief Kade) based on your input.
