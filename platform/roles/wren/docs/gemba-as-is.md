# Gemba — As-Is Flow Documentation

**Card:** #1225 | **Date:** 2026-03-09 | **Author:** Wren

## What Gemba Is

Jeff types `/gemba <role>` to watch a role work in real time. The observer translates raw tool calls into a narrative Jeff can follow. Sports commentary, not status reports.

---

## Prescribed Flow (from SKILL.md)

1. **Fast entry (<5s):** `board-ts view`, tail snapshot, declare state — 3 parallel calls, nothing else
2. **Initial read:** 2-3 sentences on what the role is doing right now
3. **Follow tail:** Background task tailing the role's session
4. **Cron loop:** `*/1 * * * *` fires every minute, observer digests new batches
5. **Commentary:** Summarize → interpret → note technique → flag product concerns → stay conversational
6. **Jeff interjects freely:** Observer responds, loop continues between interactions
7. **Exit:** Jeff says stop, card accepted/rejected, or 10-minute TTL expires
8. **Debrief:** One paragraph summary, card follow-ups, declare state back to waiting

---

## What Actually Happens (observed variance)

### Entry

| Prescribed | Actual (Wren) | Actual (Silas) | Actual (Kade) |
|-----------|---------------|----------------|---------------|
| 3 parallel calls, <5s | Often 5-10s, sometimes launches explore agents for context | Burned 2-3 min on context-building during #1220 demo (self-reported, now fixed) | Has not run gemba as observer |
| `board-ts view` only for card context | Sometimes reads the artifact too | Same pattern pre-fix | N/A |

**Gap:** All roles over-invest in context at entry. The tail IS the context — trust it.

### Commentary Style

| Prescribed | Actual (Wren) | Actual (Silas) | Actual (Kade) |
|-----------|---------------|----------------|---------------|
| Sports announcer energy | Closer to PM analysis — interprets product impact, asks "should we card this?" | Technical play-by-play — names tools, patterns, friction accurately | N/A |
| 2-3 lines per digest | Sometimes 5-8 lines when a pattern is interesting | Generally concise | N/A |
| Interpret WHY, not describe WHAT | Good at why, but sometimes slips into narrating tool calls | Good at naming technical patterns, less product framing | N/A |

**Gap:** Wren and Silas have different lenses (product vs. architecture). That's actually useful — but Jeff should know which lens he's getting. The observer should name their lens: "PM lens: ..." or "Architecture lens: ..."

### Loop Behavior

| Prescribed | Actual (Wren) | Actual (Silas) |
|-----------|---------------|----------------|
| Cron-driven, self-sustaining | Earlier: required Jeff to re-invoke. Now: cron available but not consistently used | Shipped cron pattern in SKILL.md rewrite |
| 15s digest (original) → 1min cron (current) | Mixed — sometimes polls manually, sometimes forgets | Cron-based when working correctly |
| Jeff doesn't need to re-trigger | Jeff has had to say "keep observing" multiple times | Less friction after #1208 |

**Gap:** The cron loop is new (Silas shipped it today). Not yet battle-tested across roles. Wren hasn't used the cron pattern yet.

### Jeff's Interventions (things Jeff shouldn't have to say)

These are quotes or paraphrases from actual sessions:

- "I thought gemba would loop — maybe poll 15s, comment/wait for 15s, poll again" — loop wasn't self-sustaining
- "keep observing you just missed the demo" — observer stopped watching during a key moment
- "this play by play helps me and i think is valuable for you all too" — affirming the pattern works when done right
- Jeff redirects to stay on the observed role — observer drifts into own work or analysis

**Pattern:** Jeff's interventions are mostly about **continuity** (keep watching) and **focus** (watch them, not your own thoughts).

### Exit

| Prescribed | Actual |
|-----------|--------|
| 10-min TTL, auto-debrief | New — not yet tested in practice |
| Clean exit: stop cron, stop tail, debrief | Sometimes abrupt — Jeff moves on and observer doesn't formally close |
| Debrief paragraph | Inconsistent — sometimes detailed, sometimes skipped |

---

## Where Roles Diverge

| Dimension | Wren | Silas | Kade |
|-----------|------|-------|------|
| **Entry speed** | Slow (context-seeking) | Was slow, now fixed | Never observed |
| **Commentary lens** | Product impact, user value | Technical patterns, infrastructure | N/A |
| **Loop discipline** | Manual polling, sometimes forgets | Cron-driven (new) | N/A |
| **Jeff attention cost** | Moderate — needs "keep going" nudges | Lower after #1208 fixes | N/A |
| **Debrief quality** | Good when done, sometimes skipped | Consistent | N/A |

---

## Target State (what gemba should look like every time)

### Entry (all roles, <5 seconds)
1. Three parallel calls: `board-ts view` (if card ID), tail snapshot, declare state
2. Print 2-3 sentence read of what's happening RIGHT NOW
3. Nothing else. No file reads, no agents, no artifact exploration.

### Loop (all roles, self-sustaining)
1. Background tail running
2. Cron fires every minute
3. Each digest: 2-3 lines max
4. Format: `[HH:MM] <observation>. <interpretation>. <product/arch flag if any>.`
5. If quiet: `[HH:MM] Quiet — may be thinking or waiting on Jeff.`
6. Between fires: respond to Jeff naturally, loop continues

### Commentary (all roles, same structure)
1. **What happened** — one sentence, translated from tool calls to human action
2. **What it means** — stuck, pivoting, in flow, fighting friction
3. **Flag** — product concern, arch concern, or "no concern"
4. Name your lens explicitly if it matters: "PM lens:" or "Arch lens:"

### Exit (all roles, same sequence)
1. Trigger: Jeff says stop, card event, or 10-min TTL
2. Stop cron + stop tail (clean up resources)
3. Debrief: one paragraph — what happened, patterns, time spent
4. Card any follow-ups surfaced during observation
5. Declare state: `role-state.sh <role> waiting`

---

## What This Document Does NOT Cover

- **Demo flow** — separate document (Silas documenting as-is for /demo)
- **Demo observer mode** — now collapsed into gemba (per Silas's #1208 changes)
- **Nudge reliability** — separate card #1205
- **Kade as observer** — no data yet. When Kade runs gemba, document what happens.
