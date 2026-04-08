# Three Proposals: Clearing Voice Tuning, Clearing Mobile, Seeds Pipeline

**From:** Wren
**Date:** 2026-02-22
**Cards:** C#37 (voice tuning), C#36 (mobile access), #126 (Seeds)
**Status:** Draft for Jeff review

---

## 1. Clearing Voice Tuning — C#37 (P1)

### Problem

Haiku roles in The Clearing sound generic — "sprints," "deploy windows," "decision matrices." They don't sound like Wren, Silas, or Kade. They restate each other, escalate word count each turn, and keep talking after Jeff says stop.

### Root Causes (from code review)

1. **System prompts are thin.** Each role gets ~100 words of personality. Haiku is a smaller model — it needs stronger grounding to stay in character. (`participants.ts` lines 17-66)
2. **No team memory.** Roles don't know about recent decisions, active cards, or the team's actual vocabulary (Werk, vertebrae, Clearing). They fill gaps with corporate defaults.
3. **Temperature not set.** SDK default is ~1.0. Higher temperature = more creative = more likely to drift from character. (`participants.ts` line 98+, no temperature param)
4. **No stop detection.** When Jeff types "stop" or "done," the sequential loop continues through remaining roles. No keyword check in the message handler. (`server.ts` lines 105-164)
5. **Token counts climb unchecked.** Max is 300 tokens per response, but there's no enforcement that later turns should be shorter than earlier ones. Each role sees the growing transcript and matches its length.

### Proposed Changes

**A. Context injection from /chorus (the big one)**

Before the first AI response in a session, query the Chorus context service for:
- **Voice samples:** 3-5 recent messages per role from actual Claude Code sessions. "This is how Wren actually talks."
- **Active work:** Current board state (Now items from both boards). So roles talk about real cards, not hypothetical sprints.
- **Recent decisions:** Last 5 DEC entries. So roles use the team's actual vocabulary.
- **Tone rules:** The 50-word rule, don't-restate rule, stop-when-told rule — extracted from CLAUDE.md and injected as hard constraints.

This gets injected into each role's system prompt under a `## Team Context` section. Not the full CLAUDE.md — a **distilled context packet** (~500 tokens per role) tuned for Haiku's context window.

**Implementation:** New function in `participants.ts` that calls the Chorus HTTP wrapper (C#30, already on the board) or queries the SQLite DB directly. Runs once at session init, cached for the session.

**Silas consultation needed:** Is querying the Chorus SQLite DB directly from the Clearing server viable? Or should we wait for the HTTP wrapper (C#30)? Direct DB access is simpler but couples the Clearing to the index schema.

**B. Temperature tuning**

Add `CLEARING_TEMPERATURE` env var, default to `0.7` (lower = more consistent, less drift). Current unset value (~1.0) gives Haiku too much room to improvise personality.

**C. Hard stop detection**

In `server.ts` message handler, before entering the role response loop:
```
if message matches /\b(stop|done|enough|hold)\b/i → skip role responses, emit 'round:skipped'
```
Jeff says stop, the system stops. No more orphaned responses.

**D. Descending token budget**

Instead of flat 300 tokens per role, use descending limits:
- First responder: 250 tokens
- Second responder: 200 tokens
- Third responder: 150 tokens

Forces later roles to be more concise. Discourages restating what earlier roles said.

**E. Moderator-first sequencing**

Wren always responds first (unless @-mentioned otherwise). Wren's response frames the conversation; Silas and Kade respond to Wren's framing, not independently to Jeff's message. This mirrors the moderator protocol from the bridge.

### Effort Estimate

- A (context injection): Medium — needs Chorus query integration, prompt engineering, testing
- B (temperature): Trivial — one env var
- C (stop detection): Small — regex check + early return in message handler
- D (descending budget): Small — dynamic max_tokens per role index
- E (moderator-first): Small — sort respondingRoles to put Wren first

### Recommendation

Ship B + C + D + E immediately (all small changes, high impact). Design A properly with Silas consultation, ship in second pass.

---

## 2. Clearing Mobile Access — C#36 (P2)

### Problem

Jeff can't reach The Clearing from his iPhone. Two gaps: localhost-only binding (ADR-012) and no standalone launcher (requires Claude Code session).

### Proposed Changes

**A. LAN binding with auth gate**

Bind the Clearing server to `0.0.0.0` (all interfaces) BUT add a simple auth gate:
- On first connection, show a PIN entry screen
- PIN set via `CLEARING_PIN` env var (4-6 digit numeric)
- Session cookie persists auth for the browser session
- This respects the spirit of ADR-012 (nothing exposed without auth) while enabling LAN access

**B. Persistent launcher service**

Instead of on-demand startup from Claude Code, run a lightweight launcher:
- Tiny Express server on a fixed port (e.g., 3460)
- Single page: "Start a Clearing session" button + optional context field
- On click: spawns the Clearing server, redirects to it
- Shows active session link if one is already running
- Bookmarkable on iPhone home screen

**Alternative (simpler):** Make the Clearing server itself persistent (always running) instead of on-demand. Jeff opens `http://192.168.86.36:3460/` on his phone → he's in a Clearing session. Session ends when he clicks "End Session," server stays alive for next time.

**Silas consultation needed:**
- Persistent service vs. on-demand launcher — which fits the infrastructure model better?
- Auth approach — PIN good enough, or should we use the existing SOLID auth?
- Memory/resource cost of an always-running Clearing server (it's just Express + Socket.IO, no AI until someone connects)

### Recommendation

Start with persistent server + PIN auth. Simplest path to "Jeff bookmarks URL on phone, taps it, talks to the team."

---

## 3. Seeds Capture Flow — #126 (P1)

### Problem

SMS/Seeds capture routed to Slack channels for team visibility. With Slack deprecated, Seeds land in pods but roles never see them. Jeff's primary capture flow (text on walks → triage → act) is disconnected from the team.

### Current State (from audit)

The pipeline is intact through storage:
- **Working:** Twilio → Express (`/api/capture/sms`) → SOLID pods (`/pods/jeff/capture/`)
- **Working:** Triage UI at `/admin/capture` — lists pending captures, routes to destinations
- **Working:** Pod destinations — ideas, projects, garden, rooms, glimmers, reading-list, watch-list
- **Broken:** Slack destinations (slack-wren, slack-silas, etc.) — Slack being deprecated
- **Missing:** Seeds not indexed in Chorus context service — roles can't discover them

### Proposed Changes

**A. Replace Slack routing with Chorus index routing**

Instead of posting Seeds to Slack channels, index them directly into the Chorus context service:
- New routing destination: `chorus` (replaces all 5 slack-* destinations)
- On route: write the Seed to the Chorus SQLite FTS5 index with:
  - `source: 'seed'`
  - `channel: 'capture:sms'`
  - `role: 'jeff'` (or mapped sender name)
  - `content: capture text + metadata`
- Roles discover Seeds on session start via `/chorus` reconciliation (already happens)

**B. Auto-index on capture (skip manual triage for team visibility)**

Current flow: capture → pending → manual triage → route. This means Seeds sit invisible until Jeff triages them.

Proposed: capture → auto-index to Chorus (team can see) → pending for triage (Jeff routes to final destination later).

Two-phase: immediate Chorus visibility, deferred pod routing. Jeff still triages to final destination (ideas, projects, etc.) but the team sees the Seed immediately.

**C. New triage destinations (replace Slack)**

Replace the 5 Slack destinations with:
- `team` — indexes to Chorus with `channel: 'capture:team'` (all roles see it)
- `wren` — indexes with `channel: 'capture:wren'` (Wren-specific)
- `silas` — indexes with `channel: 'capture:silas'`
- `kade` — indexes with `channel: 'capture:kade'`

Same routing UX, different backend. Triage dropdown looks the same to Jeff.

**D. Seeds visible in The Clearing**

With Seeds in the Chorus index, The Clearing's context injection (from Proposal 1A above) can surface recent Seeds in session context. "Jeff texted this 2 hours ago" becomes part of what roles know when a Clearing session starts.

### Implementation Path

1. Add Chorus index write to capture handler (new routing destination)
2. Add auto-index on capture (dual-write: pod + Chorus)
3. Replace Slack triage destinations with Chorus-backed equivalents
4. Remove Slack dependencies from capture handler
5. Test: Jeff sends SMS → appears in `/chorus search` → roles see it on session start

**Silas consultation needed:**
- Schema for Seed entries in Chorus index — extend existing schema or new source type?
- Auto-indexing write path — should it go through the Python indexer or direct SQLite from Node?

**Kade builds:** The capture handler changes are in the app repo — Kade's vertical. I'll spec, he implements.

### Recommendation

Ship A + B first (Chorus integration). Then C (triage UI update). D comes free once Proposal 1A ships.

---

## Cross-Cutting: These Three Connect

The thread running through all three proposals:

1. **Voice tuning** makes The Clearing usable by giving Haiku real context
2. **Mobile access** makes The Clearing reachable from anywhere
3. **Seeds routing** makes Jeff's capture flow visible to the system

Together: **Jeff texts himself on a walk → Seed auto-indexes to Chorus → Jeff opens The Clearing on his phone → roles already know what he captured → conversation starts with context, not from zero.**

That's the single-piece-flow vision. Each proposal is independently valuable, but together they close the loop.

---

## 4. Clearing Close-Time Execution (added per Jeff feedback)

### Problem

Intake items from Clearing sessions sit in `~/.chorus/intake/` until Wren manually processes them on next session start. That's too late — 15 items were pending this morning from yesterday's sessions. Jeff's direction: "Session start is way too late — it has to be done on the synchronous response."

### Proposed Change

The Clearing `endSession` function already runs: extract → index → exit. Extend it to: **extract → index → route → brief → exit.**

After indexing to Chorus, before process exit:
1. Run `chorus-capture.sh` to create intake items (already happens)
2. Auto-create workflows for decisions with clear role assignments
3. Write handoff briefs to role `briefs/` directories
4. Mark intake items as `routed`

All synchronous. When a role opens their next session, the work is already queued — not sitting in an intake pile.

### What Stays Manual

Ambiguous items (no clear role assignment, needs product judgment) stay as pending intake for Wren to triage. Only items with explicit role + action get auto-routed.

### Effort

Medium — needs decision/commitment parsing improvements and workflow.sh integration in the Clearing server's endSession function.

---

## 5. Permission Prompt Logger — C#38

### Problem

Jeff gets blocked hitting "enter" to approve tool calls across all three role sessions. He can't walk away. Every permission prompt is friction that tethers him to the keyboard. We don't know which calls are blocking most frequently.

### Proposed Change

Log every permission prompt event:
- Which role triggered it
- What tool + arguments
- Timestamp
- Whether Jeff approved or denied

After a few sessions, analyze the log to identify patterns. Update permission profiles (`chorus/permissions/`) to auto-allow the routine cases.

### Implementation

Hook into Claude Code's permission system — likely a PreToolUse hook that logs before prompting. Silas built the permission profiles; he knows the hook point.

### Goal

Jeff should be able to say "go" and walk away. Every permission prompt that pulls him back is failure demand.

---

## Next Steps

1. Jeff reviews this proposal
2. Silas consultation brief on technical questions (I'll send after Jeff approves direction)
3. Quick wins ship first: temperature, stop detection, descending budget, moderator-first (all in Clearing source)
4. Close-time execution added to Clearing endSession
5. Seeds Chorus integration spec → Kade brief
6. Mobile access spec → Silas brief
7. Permission prompt logging → Silas consultation
