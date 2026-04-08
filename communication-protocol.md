# Team Communication Protocol — DEPRECATED

> **DEPRECATED as of 2026-03-27.** Slack-based communication replaced by The Clearing (localhost:3470), nudges (messaging tier, port 3475), and briefs. See `team-architecture.md` for current communication model.

**Version**: 1.0
**Date**: 2026-02-15
**Decision**: DEC-016

---

## Problem

Jeff is acting as a message bus between three AI roles. When he forgets to relay something, information gets lost. Roles post to Slack but don't read it mid-session, so conversations are one-way announcements. The "blind between roles" gap means each role only sees what Jeff remembers to share.

## Principle

**Roles talk to each other through Slack. Jeff participates when he wants to, not because he has to.**

---

## Channels & Their Purpose

| Channel | Purpose | Who posts | Who reads |
|---------|---------|-----------|-----------|
| `#all-gathering` | Active conversation, cross-role coordination, testing, questions | Everyone | Everyone |
| `#decisions` | Decision announcements (mirrors decisions.md) | Wren | Everyone |
| `#wren` | PM-specific briefs, requests for Wren | Silas, Kade, Jeff | Wren |
| `#silas` | Architecture questions, requests for Silas | Wren, Kade, Jeff | Silas |
| `#kade` | Engineering briefs, build requests for Kade | Wren, Silas, Jeff | Kade |
| `#standup` | Daily status (what I did, what's next, blockers) | Everyone | Everyone |

**Key distinction**: `#all-gathering` is for conversation. Role channels are for directed requests. Both are read by the recipient.

---

## When to Check Slack

### Every Conversation Turn (RULE)
**Before responding to any user message**, check `#all-gathering` and your role channel. This is not optional. Do it first, before any other work. This ensures you're current on cross-role conversation even when Jeff is your primary interaction.

### Session Start
- Read `#all-gathering` (last 20 messages)
- Read your role channel (last 10 messages)
- Note anything that needs a response — respond before starting new work

### While Waiting for Next Prompt (RULE)
When you've responded and are waiting for Jeff's next message, use that moment to:
- Check your `briefs/` directory for new or unread briefs
- Check your role channel for direct requests
- Check `#all-gathering` for cross-role conversation
- Check `board.sh list` if it's been a while

This turns idle time into awareness time. You won't miss a brief or a Slack message just because you were waiting.

### Mid-Session Breakpoints
Check `#all-gathering` and your role channel at these natural breakpoints:
- **Before starting a new task** — someone may have posted context that changes your approach
- **After completing a task** — check if anyone responded to your earlier posts
- **When Jeff mentions another role** — they may have posted something relevant

### Active Collaboration Mode
When Jeff says he's working with another role (e.g., "I'm testing with Kade"), or when multiple roles are active simultaneously:
- Check `#all-gathering` **frequently** — every few minutes / every couple of actions
- Respond to questions directed at you
- Post observations and findings as they happen, not batched later
- This is conversation, not announcements

---

## How to Address Messages

### Directing a message to a specific role
Prefix with the role name:
- `Kade — can you check the capture endpoint logs?`
- `Silas — does this align with the ontology?`
- `Wren — should we defer this to next sprint?`

### Directing a message to Jeff
Prefix with Jeff:
- `Jeff — need your input on backup destination before I can build this`

### General announcements (no specific recipient)
Just post. Everyone reads `#all-gathering`.

### When you need a response
Say so explicitly:
- `Silas — need your take on this before I can proceed. Checking back in 5.`

---

## What Goes Where

| Type of communication | Channel | Also goes to |
|-----------------------|---------|-------------|
| Quick question to a role | Their role channel | — |
| Status update (shipped, found bug) | `#all-gathering` | activity.md |
| Active testing/debugging conversation | `#all-gathering` | — |
| New brief ready for review | Role channel + `#all-gathering` | briefs/ directory |
| Decision made | `#decisions` + `#all-gathering` | decisions.md |
| Blocker or request for Jeff | `#all-gathering` | — |
| Daily standup | `#standup` | — |

### Slack vs Briefs vs Activity Log

- **Slack**: Conversation, questions, quick updates, active collaboration. Ephemeral — assume messages scroll off.
- **Briefs** (`briefs/` directories): Detailed specs, architectural reviews, build plans. Permanent — the source of truth for work specs.
- **Activity log** (`activity.md`): Permanent record of what happened. Append-only. Every significant action gets a line.

**Rule**: If it matters in a week, it goes in a brief or the activity log. Slack is for right now.

---

## Jeff's Role in Communication

Jeff participates when he wants to — not because the system breaks without him.

- Jeff can jump into `#all-gathering` anytime
- Roles should NOT wait for Jeff to relay messages between them
- If a role needs input from another role, post to their channel and to `#all-gathering`
- If a role needs Jeff's decision, post to `#all-gathering` and say so explicitly

**Jeff is the owner, not the router.**

---

## Scripts

```bash
# Post a message
../../scripts/slack-post.sh <channel> "<message>"

# Read recent messages
../../scripts/slack-read.sh <channel> [count]

# Quick catch-up (all channels)
../../scripts/slack-read.sh all-gathering 20
../../scripts/slack-read.sh <your-channel> 10
```
