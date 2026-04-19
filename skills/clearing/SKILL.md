---
name: clearing
description: Start a Clearing session — multi-party chat with Jeff and all three AI roles (Wren, Silas, Kade) in a browser window. Use when alignment across roles is needed.
user-invocable: true
---

# The Clearing — Multi-Party Chat

This skill creates a Clearing session on the always-on clearing service (localhost:3460). Roles arrive warm — aware of board state, recent decisions, and team activity via Chorus context injection.

## How to Launch

### Step 1: Build context

Assemble Chorus context by running these in parallel:

```bash
# Board state (WIP + Now)
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards list 2>/dev/null | head -30

# Recent decisions (last 10)
grep -E '^## DEC-' /Users/jeffbridwell/CascadeProjects/chorus/roles/wren/decisions.md | tail -10

# Recent team activity
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-query.sh reconcile 2>/dev/null | head -60
```

Compose a context string from these results plus your current session knowledge — what's being discussed, what decision is needed. **Don't ask Jeff to type context.** When Jeff specifies a topic, weave it in.

### Step 2: Create session and open browser

```bash
# POST context to create a session
SESSION_ID=$(curl -s http://localhost:3460/api/sessions \
  -X POST -H 'Content-Type: application/json' \
  -d '{"context":"YOUR_CONTEXT_HERE"}' | python3 -c "import sys,json; print(json.load(sys.stdin).get('sessionId',''))")

# Open in Chrome
open -a "Google Chrome" "http://localhost:3460"
```

**Important**: The service is always running — no spinup wait, no terminal blocked. Just POST and open.

### Demo mode — `/clearing demo <card-id>`

For card demos (DEC-048 proving gate). Auto-assembles card context:

```bash
# Get card details
CARD=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view <card-id> 2>/dev/null)
```

Build context from card details + recent commits + briefs, then POST to `/api/sessions` with a demo protocol context that enforces: **builder presents → PM verifies AC → Jeff reacts → accept/reject/iterate.**

## After the Session

When Jeff ends the session, read the results:

```bash
cat /tmp/clearing-last-return.json
```

### What to Report

1. **Decisions** — list any DECISION markers captured. Most important output.
2. **Session summary** — message count, participants, cost, duration.
3. **Action items** — commitments or next steps from the transcript.
4. **Card updates** — if decisions affect kanban cards, update them.
5. **Brief routing** — if work was produced for another role, send the brief.

## When to Use

- **Demo**: Jeff says "demo", "show me", or a card needs proving gate → use demo mode
- **Alignment**: Multi-role alignment needed on a decision
- **Team**: Jeff asks to "talk to the team" or "get everyone's input"
- **Real-time**: Back-and-forth is better than async briefs
- **Cross-role**: Need architectural + engineering input simultaneously

## Service Details

- **Port**: 3460 (always-on via LaunchAgent `com.chorus.clearing`)
- **Health**: `curl http://localhost:3460/`
- **Model**: Haiku by default (~$0.10 per session)
- **Context**: Injected per-session via POST, appears in every role's system prompt + UI banner
