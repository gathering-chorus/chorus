# Team Work Cadences — Analysis & Proposal (#1397)

**From**: Wren (PM) | **Date**: 2026-03-15 | **For**: Silas, Kade review → then Jeff demo

## What the data says

**31 days analyzed** (Feb 13 - Mar 15). 290 sessions, 287 cards completed, 665 briefs, 89 decisions.

### Three natural rhythms already exist

**1. The Session Rhythm (2-5 hours)**
The atomic unit of work. Each role runs 2-5 sessions/day, averaging 9.3 sessions/day across all three. Sessions end at natural breakpoints — context window fills, major card ships, or Jeff redirects. This is the team's heartbeat.

**2. The Build-Clean Cycle (3-5 days)**
Build waves (3-5 days of feature work) followed by 1-2 days of consolidation/hardening. Observable pattern:
- Mar 1-4: Build wave → Mar 5: explicit "clear then harden" from Jeff
- Mar 6-8: Mixed build+harden → Mar 9: review/demo day
- Mar 10-11: Build+consolidation → Mar 12-14: massive build wave
- **Cruft lag: 0-4 days, typically 1.** Every 2-3 build cards generates 1 clean card.

**3. The Role Grain**
Each role has a distinct build:clean ratio that reflects their domain:
- **Kade**: 61% build / 39% clean — feature-forward, clean trails behind
- **Silas**: 29% build / 71% clean — infrastructure keeper, hardening is the work
- **Wren**: 40% build / 60% clean — PM hygiene (board triage, state files, briefs) IS the work

### What triggers clean work (not a calendar)

| Trigger | Response | Lag |
|---------|----------|-----|
| Disk pressure (>90%) | Silas immediate cleanup | 0 days |
| Crash cascade (harvest OOM, NFS stall) | Same-session fix | 0 days |
| Jeff says "harden" or "clear" | Phase shift, all roles | 0 days |
| Post-demo polish | Fix/polish cards from Jeff's feedback | Same session |
| Accumulation threshold | ~15 stale items triggers Wren board triage | 3-5 days |
| V1/milestone boundary | Wren assessment → Silas hardening batch | 1-2 days |

### Jeff's attention pattern

| Pattern | % | Meaning |
|---------|---|---------|
| Direction | 33% | Jeff leads with intent, roles execute |
| Gemba | 24% | Jeff watches roles work — observation IS management |
| Demo | 16% | Proving gate — Jeff sees the work live |
| Reflection | 9% | Meta-process thinking |
| Story | 8% | Personal context sharing — feeds product |
| Ideation | 6% | "What if..." — generates cards |
| SWAT | 3% | Rare — system is mostly stable |

Jeff navigates by reading conditions, not by schedule. Direction + Gemba + Demo = 73% of his attention. He's a navigator, not a scheduler.

### Cross-role coordination

- **21.5 briefs/day** — primary coordination mechanism
- **2.3 gemba walks/day** — Jeff's observation practice
- **Wren creates 78% of cards** — PM is the intake funnel
- **Clearings are rare** — most coordination is async (briefs + nudges)

---

## Proposal: Three Cadences, No Calendar

Jeff explicitly said "not agile or xp." The data confirms the team doesn't work on calendar rhythms. It works on **condition-triggered rhythms**.

### Cadence 1: The Pulse (every session)
**What**: Each session starts with context load, operates with card-first discipline, ends with state capture.
**Already happening**: SessionStart hooks, chorus prompt, close-out sequence, activity.md logging.
**Improve**: Add cruft-velocity check to session start — "N clean cards in queue, ratio is X:Y." Surface the accumulation before it becomes a problem.

### Cadence 2: The Sweep (condition-triggered, not calendar)
**What**: When cruft accumulates past a threshold, declare a sweep. All roles shift to clean work until the ratio recovers.
**Triggers** (any one):
- Board has >10 untagged or stale cards
- Build:clean ratio exceeds 3:1 for any role over 3 sessions
- Disk usage >80%
- Jeff says "harden" or "clear"
- Post-milestone (V1 boundary, major feature complete)
**Duration**: 1-2 sessions, not days. The data shows clean work is fast when focused.
**Instrument**: Track build:clean ratio per role per session. Surface in `/werk`.

### Cadence 3: The Reflection (weekly-ish, Jeff-triggered)
**What**: Jeff steps back and looks at the whole. Not a standup — a "how are we working" conversation.
**Already happening organically**: Jeff's Reflection pattern (9% of interactions) + Story pattern (8%). Combined 17% — roughly 1 in 6 interactions.
**Improve**: Don't formalize this. Jeff's reflections come when they come. But Wren should **prompt** reflection when it's been >5 days since the last one. One question: "What are you noticing about how we're working?"

---

## What NOT to do

- **No sprints.** The data shows variable-length build waves (3-5 days), not fixed time boxes.
- **No standups.** The session boot sequence IS the standup. Activity.md IS the status report.
- **No velocity tracking.** Card count is misleading — Silas's 14-card Docker migration day ≠ Kade's 2-card harvest pipeline day. Both were full days of work.
- **No retrospectives.** Jeff's reflection pattern is better — it happens in context, not in ceremony.

---

## Metrics to track going forward

| Metric | Where | Why |
|--------|-------|-----|
| Build:clean ratio per role | `/werk` dashboard | Surface accumulation early |
| Cruft queue depth | Board scan | Trigger sweep cadence |
| Session count per day | Spine events | Baseline for workload |
| Jeff redirect rate | Interaction patterns | Are roles self-directing enough? |
| Demo scorecard (auto-accept %) | Close-out | Is quality improving? (baseline: 17%) |

---

*Ready for team review. Silas and Kade: push back on anything that doesn't match your experience. Jeff gets the final demo after your feedback.*
