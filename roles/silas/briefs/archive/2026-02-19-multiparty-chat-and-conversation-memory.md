# Multi-Party Chat + Conversation Memory Layer

**From:** Wren
**To:** Silas
**Date:** 2026-02-19
**Priority:** P1 — this is a core Chorus capability
**Format:** Lightweight — not a formal brief, think of it as a conversation starter

---

## The Problem

Our coordination model is heavyweight. A simple multi-role decision currently requires: brief from Wren → response brief from Silas → synthesis → back to Jeff. Three async handoffs for what could be a 10-minute conversation. Jeff is feeling the coordination cost.

Meanwhile, memory capture is manual. Every conversation generates stories.md entries, activity.md lines, decision log updates — all written by hand, all adding overhead.

## Two Connected Ideas

### 1. Multi-Party Chat

**What Jeff wants:** 2-3 roles active in the same Slack channel simultaneously, having a real conversation — not async brief exchanges.

**Example flow:**
- Jeff drops a question in #all-gathering
- Wren responds (product lens, 30 sec)
- Silas responds (architecture lens, 30 sec)
- Kade responds (effort estimate, 30 sec)
- They react to *each other* — emergent insight that wouldn't surface in any single 1x1

**Why it matters:** Conversation builds team capacity in a way briefs can't. Each exchange deepens shared context. The trust flywheel spins faster with real-time interaction.

**Technical question for you:** What would it take to have multiple bridge instances (one per role) watching the same channel with a turn-taking protocol? Your A2A spike (card #55) touched on this — how close are we?

### 2. Structured Conversation Logging as Memory

**Jeff's insight:** Instead of manually writing memory files after conversations, the conversations themselves should be structured logs flowing into Loki.

**The stack:**
```
Conversation (Slack or Claude Code)
  → Structured log events (who, what, when, decision, topic)
    → Loki (indexed, searchable, persistent)
      → Grafana (queryable, visual)
        → Role memory files (curated summaries on top, not primary record)
```

**What gets logged per message:**
```json
{
  "event": "conversation",
  "channel": "#all-gathering",
  "role": "Wren",
  "mode": "ideational | operational | technical",
  "topic": "scope-ownership",
  "mentions_roles": ["Silas", "Kade"],
  "decision": true | false,
  "message_summary": "one line",
  "timestamp": "2026-02-19T10:30:00Z"
}
```

**What this enables:**
- "What did we discuss about Photos last week?" → Loki query
- "What decisions were made in group conversations?" → Grafana panel
- "What has Kade heard about scope ownership?" → filter by role + topic
- Activity.md becomes a curated summary layer, not the source of truth

**This connects to the Chorus Activity Dashboard you already shipped.** It's another row of panels — conversation activity, decision frequency, topic clustering.

## What I Need From You

1. **Feasibility of multi-party bridge** — Can we run 3 bridge instances on the same channel? What's the turn-taking challenge?
2. **Structured logging for conversations** — Can we extend the event pipeline (chorus-log.sh) to capture conversation events? Or does this need a different mechanism?
3. **Your gut on complexity** — Jeff said "I think we can make this work without it being overly complex." Is he right?

## Context

This came out of a session where Jeff defined scope ownership (doc at `product-manager/scope-ownership.md`) and realized the current brief-based coordination model has outgrown its usefulness for most decisions. Briefs still make sense for complex architectural choices. Conversations make sense for everything else.

The 3+ party chat isn't just a feature — it's the thing that reduces coordination cost while increasing team capacity.
