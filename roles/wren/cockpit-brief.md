# Cockpit — Product Brief

**From:** Wren (PM)
**Date:** 2026-02-20
**Board:** Chorus #16
**Priority:** P1

---

## The Problem Jeff Is Solving

Jeff manages three AI roles across three terminal tabs plus Slack. Each role has its own context. No role can see what the other roles have done. Jeff is the only one who can see all three panes — and when he asks for status, each role gives a potentially stale answer.

**The screenshot that proved it:** Wren (bridge) said "Kade hasn't started DEC-028." Kade's terminal said "DEC-028 is already shipped." Same moment, different realities. Jeff saw both. We couldn't.

**Jeff's words:** "Until we can build something more deterministic, this lack of shared context may be worse than me just chatting with each of you individually."

**Tim Newsom reference:** Jeff's friend built a custom LLM interface (Star Trek aesthetic). Jeff's reaction: "I don't want a Star Trek console — the idea of having a way to interact with you that works sounds much needed."

---

## What the Cockpit Is

A single page Jeff opens BEFORE opening terminal tabs. It answers three questions from deterministic sources (git, board, files) — not from role opinions:

1. **What happened?** — Recent commits by role, board movements
2. **What's waiting for me?** — Pending briefs, decisions needing approval, blocked cards
3. **What's each role doing?** — Last standup, current work state, next-session.md

---

## What the Cockpit Is NOT

- **Not a Slack replacement.** Jeff already has Slack open. Don't duplicate the feed.
- **Not a Grafana replacement.** Infrastructure metrics stay in Grafana.
- **Not a terminal replacement.** Jeff still talks to each role in their tab.
- **Not a Star Trek console.** No 18-panel dashboard. No flashy real-time feeds. Clean, glanceable, factual.
- **Not another monitoring tool.** This is a *decision support* tool — "what needs my attention?"

---

## Design Principles

1. **Facts over opinions.** Git commits are facts. Board state is facts. Role bridge responses are opinions. Show facts.
2. **Reduce windows, don't add one.** If the cockpit works, Jeff opens fewer tabs — not more.
3. **Glanceable in 30 seconds.** If Jeff can't get value in 30 seconds, the design is wrong.
4. **Pull, don't push.** Jeff opens it when he wants to see state. No notifications, no alerts, no urgency signals.
5. **Deterministic data sources only.** Read from git log, Vikunja API, filesystem (briefs/, next-session.md, decisions.md). Never from Slack or role memory.

---

## Data Sources (All Deterministic)

| What | Source | How to Read |
|------|--------|-------------|
| Recent commits | `git log --since="24h" --format` | Parse: role prefix, message, timestamp |
| Board state | Vikunja API (`/api/v1/projects/2/views/.../tasks`) | Filter by bucket (Now/Next/Blocked) |
| Chorus board | Vikunja API (project 4) | Same pattern |
| Pending briefs | Filesystem: `ls -lt */briefs/*.md` | Count unread, show newest per role |
| Decisions | `product-manager/decisions.md` | Parse: last N decisions, status |
| Role state | `*/next-session.md` (DEC-028 Path C) | Parse: in-progress, pending, commitments |
| Session cost | `messages/cost-log.md` | Last entry per role |

---

## Sections (Ordered by Jeff's Workflow)

### 1. "What needs my attention?" (Top)
- Briefs waiting for Jeff's input (not role-to-role briefs — only ones addressed TO Jeff or requiring his decision)
- Blocked cards on either board
- Decisions in "pending" or "in progress" status

### 2. "What shipped?" (Middle)
- Git commits from last 24h, grouped by role
- Cards that moved to Done today
- Color-coded by role (Wren green, Silas amber, Kade blue)

### 3. "What's in flight?" (Bottom)
- Cards in "Now" column on both boards
- Each role's current work from next-session.md
- Auto-refresh every 60s (same as current Chorus page)

---

## What Success Looks Like

Jeff opens the cockpit at the start of the day. In 30 seconds he knows:
- DEC-028 shipped (he sees the commit)
- Silas is writing the cockpit architecture brief (he sees the card)
- There's a brief in Kade's inbox he hasn't read yet
- No blocked cards

Then he opens whichever terminal tab needs his attention first. He doesn't have to ask "what's the status?" because he already knows.

---

## Build Approach

This is a page in the Gathering app (`/cockpit` or enhanced `/chorus`). Not a new service. It reads from existing APIs and filesystem. Kade builds it; Silas defines the data contracts and refresh strategy.

**Smallest first version:** Static page that reads git log + board state + briefs count. No login required (it's on localhost). Ship in one session, iterate from Jeff's feedback.

---

## Open Questions for Jeff

1. Should this replace /chorus or be a separate /cockpit route?
2. Do you want session cost visible here? (Running total, daily burn)
3. Should it show Slack activity at all, or strictly deterministic sources only?

---

— Wren
