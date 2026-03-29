# Demo (Proving Gate) — As-Is Flow Documentation

**Card:** #1208 | **Date:** 2026-03-09 | **Author:** Silas

## What Demo Is

The proving gate (DEC-048). Builder finishes a card, shows it working to Jeff, gets acceptance. No self-service Done for code changes.

---

## Prescribed Flow (from SKILL.md)

1. **Validate** — card exists, is in WIP, has AC
2. **Smoke check** — builder walks happy path, verifies it works
3. **Prep brief** — structured summary: stakes, not mechanics
4. **Signal** — `board-ts demo`, spine event, nudge observers
5. **Show** — open the thing, let Jeff see it, pause, follow his attention
6. **Accept or reject** — Jeff/Wren decides, `board-ts done` or `board-ts reject`

---

## What Actually Happens (observed variance)

### Step 1: Validate

| Prescribed | Actual |
|-----------|--------|
| Check card is in WIP with AC | Usually skipped — builder knows what they're working on |
| AC drives the demo | AC sometimes missing or vague; builder demos what they built, not what was asked |

**Gap:** When AC is thin, the demo has no acceptance criteria to verify against. Wren writes AC (DEC-048) but sometimes it's "make X work" without observable outcomes.

### Step 2: Smoke Check

| Prescribed | Actual |
|-----------|--------|
| Walk the happy path before demo | Inconsistent — sometimes done, sometimes builder discovers bugs during demo |
| Report PASS/FAIL | Verbal, not logged. No spine event for smoke result |
| Fail = stop, fix first | Sometimes builder pushes through a partial demo anyway |

**Gap:** No `smoke-check.sh` script. No automation. No pass/fail event. The smoke check is honor-system.

### Step 3: Prep Brief

| Prescribed | Actual |
|-----------|--------|
| Structured brief: stakes, not mechanics | Sometimes done well (Wren's #1220 had a clean brief), sometimes improvised |
| Write to Wren's briefs/ directory | Inconsistent — sometimes written, sometimes verbal |
| Lead with why it matters | Builders often lead with what they changed, not why it matters |

**Gap:** Brief generation is manual. Template exists in SKILL.md but isn't enforced or auto-populated from card data.

### Step 4: Signal

| Prescribed | Actual |
|-----------|--------|
| `board-ts demo <id>` | Sometimes run, sometimes skipped. Emits spine event only — no orchestration |
| Nudge observers | Manual. Builder must remember to nudge each role separately |
| Observers enter gemba | Observers didn't auto-enter until today's SKILL.md update. Required Jeff to invoke `/gemba` |

**Gap:** `board-ts demo` is a thin event emitter. It should orchestrate: nudge observers, open the page, prep the brief. Today it just logs `card.demo.started`.

### Step 5: Show

| Prescribed | Actual |
|-----------|--------|
| Open the thing, let Jeff see it | Usually works — builder opens Chrome to the right page |
| Pause, wait for Jeff | Builders sometimes over-explain before Jeff has processed what's on screen |
| Follow Jeff's attention | Good when it happens. Jeff redirects ("show me X") and builder follows |
| Use demo-scroll.sh for long pages | Scroll granularity is coarse — 3-4 scroll+look cycles to navigate a page |
| Use /look to verify viewport | Works but adds latency. Each look is a screenshot + vision read |

**Friction points observed in #1220 demo:**
- Jeff said "don't see pages" — builder had edited the file but Jeff was looking at a different scroll position
- Jeff said "that's text not a page sample" — builder's mental model (text tables) didn't match Jeff's (visual examples)
- Jeff said "embed the page so I can see dynamic not static" — two pivots needed to land on iframes
- Three scroll+look cycles to find the right section on a long page

**Gap:** No full-page capture. Demo-scroll.sh works but is trial-and-error. No way to jump to a specific section.

### Step 6: Accept or Reject

| Prescribed | Actual |
|-----------|--------|
| Jeff/Wren decides | Works — Jeff gives verbal acceptance |
| `board-ts done <id>` | Sometimes immediate, sometimes delayed to session close |
| `board-ts reject <id> "reason"` | Rarely used — Jeff gives feedback and builder iterates in-session |
| Outcome brief back to Wren | Inconsistent. Sometimes written, sometimes lost |

**Gap:** No post-demo brief auto-generated. The demo outcome (what Jeff saw, his reaction, what to iterate on) should be captured automatically.

---

## Observer Behavior During Demo

Now collapsed into `/gemba`. See `product-manager/docs/gemba-as-is.md` for full documentation.

Key variance from #1220 demo:
- Silas spent 2-3 minutes on context-building instead of watching (now fixed — fast entry with card ID)
- Observer loop required Jeff to re-invoke `/gemba` (now fixed — cron-based self-sustaining loop)
- Nudge exchange limit (4 max) blocked feedback delivery (brief pathway worked as fallback)

---

## What's Wired vs. Manual vs. Missing

| Component | Status | Notes |
|-----------|--------|-------|
| `board-ts demo <id>` | **Wired** — emits spine event | Thin; no orchestration |
| `board-ts done <id>` | **Wired** — emits accepted + completed events | Works |
| `board-ts reject <id>` | **Wired** — emits rejected event | Rarely used |
| Smoke check | **Manual** — builder eyeballs it | No script, no event |
| Demo prep brief | **Manual** — template in SKILL.md | No auto-generation |
| Nudge observers | **Manual** — builder calls nudge.sh | Should be part of `board-ts demo` |
| Observer entry | **Semi-wired** — gemba skill updated today | Cron loop not yet battle-tested |
| Demo scroll | **Semi-wired** — demo-scroll.sh exists | Coarse granularity, no section jump |
| `/look` capture | **Wired** — screenshot + vision | Works but adds latency |
| Post-demo brief | **Missing** — not captured | Outcome, Jeff's reaction, iterate items |
| Full-page capture | **Missing** — no tool | Jeff asked about this during session |

---

## Jeff's Interventions (things Jeff shouldn't have to say)

From today's #1220 demo and prior sessions:

- "don't see pages" — builder and Jeff looking at different parts of the screen
- "that's text not a page sample" — builder's format didn't match Jeff's expectation
- "embed the page so I can see dynamic not static" — Jeff had to clarify twice what "example" meant
- "start looping please" — observer wasn't self-sustaining
- "nudge silas again" — nudge didn't land (busy state)
- "maybe we need a card for getting a whole page top to bottom" — demo tooling gap

**Pattern:** Jeff's interventions are about **visual alignment** (we're not looking at the same thing) and **loop continuity** (keep watching, keep commenting).

---

## Target State

### What `/demo <card-id>` should do (one command)

1. `board-ts view <id>` — get card context, verify WIP + AC exists
2. Smoke check — hit the primary endpoint/page, verify 200 or artifact exists
3. Generate demo brief from card title + AC + smoke result
4. `board-ts demo <id>` — emit spine event
5. Nudge all other roles with card ID → they auto-enter `/gemba <builder> <card-id>`
6. Open the page in Chrome for Jeff
7. Pause — wait for Jeff

### What acceptance should capture

After Jeff says accept:
1. `board-ts done <id>`
2. Auto-generate outcome brief: what was shown, Jeff's reaction, any iterate items
3. Write to `product-manager/briefs/` for Wren's record

### Automation priority

| Item | Impact | Effort |
|------|--------|--------|
| Observer auto-entry on nudge | High — eliminates Jeff re-triggering | Done (SKILL.md) |
| Fast entry with card ID | High — eliminates context-building delay | Done (SKILL.md) |
| 10-min TTL | Medium — prevents runaway demos | Done (SKILL.md) |
| Nudge auto-drain | Medium — stale nudges pile up | Carded (#1224) |
| `board-ts demo` orchestration | High — one command instead of 5 | Not yet carded |
| Smoke check script | Medium — removes honor system | Not yet carded |
| Post-demo outcome brief | Medium — captures signal that's currently lost | Not yet carded |
| Full-page screenshot | Medium — eliminates scroll+look cycles | Not yet carded |

---

## Related Documents

- `product-manager/docs/gemba-as-is.md` — observer behavior (Wren)
- `~/.claude/skills/demo/SKILL.md` — prescribed demo flow
- `~/.claude/skills/gemba/SKILL.md` — prescribed observer flow
- DEC-048 — proving gate decision
