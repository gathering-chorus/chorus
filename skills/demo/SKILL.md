---
name: demo
description: Proving gate — smoke check, prep summary, signal to Wren, builder cannot self-accept.
user-invocable: true
hooks:
  PreToolUse:
    - matcher: "Bash|Read|Write|Edit|Skill"
      hooks:
        - type: command
          command: "echo \"{\\\"ts\\\":\\\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\\\",\\\"tool\\\":\\\"$CLAUDE_TOOL_NAME\\\",\\\"skill\\\":\\\"demo\\\"}\" >> /tmp/demo-trace.jsonl"
---

# /demo — Proving Gate

When a builder (Kade or Silas) finishes a card and is ready to show it, `/demo <card-id>` runs the proving gate (DEC-048).

**Any role can invoke this.** Jeff can call it to kick off a demo. A builder calls it when they're ready.

## Arguments

```
CARD_IDS=<space-separated card IDs, or WF-NNN for a pipeline>
```

**Single card:** `/demo 1459` — standard single-card demo
**Multiple cards:** `/demo 1459 1461 1480` — multi-card demo, loop all steps for each card
**Pipeline:** `/demo WF-130` — read the manifest, demo all cards in the pipeline

If no argument given, check the role's current WIP card from andon state.

### Multi-card / Pipeline handling

When multiple card IDs or a pipeline ID is given:
1. Parse the arguments: if starts with `WF-`, read the manifest from `messages/workflows/archive/WF-NNN.json` and extract all card IDs
2. Run Steps 1-1.5-2-3-4-5 **for each card** — validate, AC pre-flight, gates, smoke check, stakes brief, signal
3. Consolidate into **one demo brief** with a section per card
4. Nudge roles **once** with the full list, not per-card: `[demo] #1459 #1461 #1480 — <pipeline name or summary>`
5. Nudge for feedback **once**: `[feedback] #1459 #1461 #1480 — <summary>. Questions? Concerns?`

## Step 1: Validate (HARD GATE)

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view ${CARD_ID}
```

**All three must be true or the demo stops:**
- Card exists
- Card is in WIP or Now status (or Done for post-acceptance demos)
- Card has AC (acceptance criteria) in description

Extract and print: card title, owner (builder), AC items, status.

If any check fails → **STOP:**
- Card not found: `Card #${CARD_ID} does not exist.`
- Wrong status: `Card #${CARD_ID} is in <status> — must be in WIP to demo.`
- No AC: `Card #${CARD_ID} has no acceptance criteria. Add AC before demo.`

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log demo.validate.completed <your-role> card=${CARD_ID} result=pass|fail
```

## Step 1.5: AC Pre-flight (HARD GATE) — #2068

**Before any gate fires, verify AC is complete and the demo brief exists.** This prevents the round-trip failure where a builder requests gate:product with unchecked AC, gets bounced, fixes it in 30 seconds, and re-requests — training the team to treat the first gate request as a dry run.

### 1. AC items all checked

```bash
AC_LINES=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view ${CARD_ID} | grep -E '^\s*- \[[ x]\]')
AC_TOTAL=$(echo "$AC_LINES" | grep -c '^\s*- \[' 2>/dev/null | tr -d '\n')
AC_CHECKED=$(echo "$AC_LINES" | grep -c '^\s*- \[x\]' 2>/dev/null | tr -d '\n')
: "${AC_TOTAL:=0}" "${AC_CHECKED:=0}"
UNCHECKED_LINES=$(echo "$AC_LINES" | grep '^\s*- \[ \]')
```

**Gate logic:**
- If `AC_CHECKED < AC_TOTAL` → **STOP.** Print the unchecked items and do NOT proceed to gate chain:
  ```
  BLOCKED: #<card-id> has <N> unchecked AC items. Complete these before demo:
  <list of unchecked items>
  ```
- If `AC_TOTAL = 0` → **STOP.** Card has no AC — same as Step 1 validation.

### 2. Post demo evidence (#2090)

**For single-card demos:** Post a card comment as gate evidence. No brief file — spine events and card comments are the provenance record.

```bash
# Single-card demo: card comment as evidence
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards comment ${CARD_ID} "demo:preflight-pass ac=${AC_CHECKED}/${AC_TOTAL} — <your-role>"
```

**For multi-card/pipeline demos:** Generate a consolidated brief file in `roles/wren/briefs/` (the consolidated view across cards is the unique value a file provides).

```bash
# Multi-card only: write consolidated brief
BRIEF_PATH="/Users/jeffbridwell/CascadeProjects/chorus/roles/wren/briefs/$(date +%Y-%m-%d)-demo-${CARD_IDS// /-}.md"
```

**Gate logic:**
- If card comment post fails → **STOP.** `Failed to post demo evidence for #${CARD_ID}.`
- If posted → print: `Demo evidence: card comment demo:preflight-pass ac=${AC_CHECKED}/${AC_TOTAL}`

### Pre-flight failure = no gates, no nudges

If either check fails, the skill prints what's missing and **does NOT** proceed to Step 2. No gate:product nudge, no gate chain, no wasted cycle.

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log demo.preflight.completed <your-role> card=${CARD_ID} result=pass|fail ac=${AC_CHECKED}/${AC_TOTAL}
```

## Step 2: Gate chain (HARD GATE)

**Before the smoke check, run the full gate chain.** This is how the team verifies quality together — every demo, every time.

**Skip conditions:** Cards tagged `type:chore` or `type:swat` skip the gate chain (crisis/housekeeping cards).

### Gate sequence

1. **Run `/gate-product <card-id>`** — AC described, experience section, domain registered, spine contract. Wren runs this. On pass, nudges Kade for gate-code. **Note:** Demo evidence (card comment `demo:preflight-pass`) already exists from Step 1.5, so the check passes on first run.
2. **Run `/gate-code <card-id>`** — tests green, build clean, no new warnings, pattern match. Kade runs this.
3. **Run `/gate-quality <card-id>`** — hooks pass, regression clean, no console.log, debt check. Kade runs this. On pass, auto-nudges Silas for arch review.
4. **Wait for `/gate-arch` pass from Silas** — system fit, namespace conventions, domain boundaries. Silas runs this after receiving the nudge.
5. **Run `/gate-ops <card-id>`** — health checks, log flow, rollback path, disk health. Silas runs this. On pass, posts comment only — chain is complete, no nudge (per #2222: demo-caller is already watching the card).

Check for gate results via card comments:
```bash
# Check which gates have passed
CARD_COMMENTS=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards comments ${CARD_ID} 2>/dev/null)
PRODUCT_PASS=$(echo "$CARD_COMMENTS" | grep -c 'gate:product-pass')
CODE_PASS=$(echo "$CARD_COMMENTS" | grep -c 'gate:code-pass')
QUALITY_PASS=$(echo "$CARD_COMMENTS" | grep -c 'gate:quality-pass')
ARCH_PASS=$(echo "$CARD_COMMENTS" | grep -c 'gate:arch-pass')
OPS_PASS=$(echo "$CARD_COMMENTS" | grep -c 'gate:ops-pass')
```

### Gate logic

**Every role starts by checking card comments for existing gate passes.** Never attempt to run a gate you don't own.

1. **Check comments first** — always, regardless of who is demoing:
   ```bash
   CARD_VIEW=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view ${CARD_ID} 2>/dev/null)
   PRODUCT_PASS=$(echo "$CARD_VIEW" | grep -c 'gate:product-pass')
   CODE_PASS=$(echo "$CARD_VIEW" | grep -c 'gate:code-pass')
   QUALITY_PASS=$(echo "$CARD_VIEW" | grep -c 'gate:quality-pass')
   ARCH_PASS=$(echo "$CARD_VIEW" | grep -c 'gate:arch-pass')
   OPS_PASS=$(echo "$CARD_VIEW" | grep -c 'gate:ops-pass')
   ```

2. **Run only gates you own** — if a gate is missing and you own it, run it inline. If you don't own it, nudge the owner:
   - `gate:product` missing → Wren runs `/gate-product`, or nudge Wren
   - `gate:code` missing → Kade runs `/gate-code`, or nudge Kade
   - `gate:quality` missing → Kade runs `/gate-quality`, or nudge Kade
   - `gate:arch` missing → Silas runs `/gate-arch`, or nudge Silas
   - `gate:ops` missing → Silas runs `/gate-ops`, or nudge Silas

3. **If gates are missing and you've nudged:** Report which gates are pending and who was nudged. **Do not block silently** — tell Jeff what you're waiting on:
   ```
   Gate chain waiting for #<card-id>:
     gate:product  PASS
     gate:code     PASS
     gate:quality  MISSING — nudged Kade
     gate:arch     PASS
   Waiting on Kade for gate:quality. Will proceed when comment appears.
   ```

- **If all five gates passed:** Print gate summary and continue to Step 3:
  ```
  Gate chain: PASS
    gate:product  PASS — Wren
    gate:code     PASS — Kade
    gate:quality  PASS — Kade
    gate:arch     PASS — Silas
    gate:ops      PASS — Silas
  ```
- **If any gate FAILED:** STOP. Print the failing gate's output. Builder fixes before re-running `/demo`.

## Step 3: Smoke check (HARD GATE)

**Run the automated smoke check first.** This is a gate — non-zero exit blocks the demo.

```bash
# For code cards that touch app pages: run smoke-check.sh (#2229 scoped)
# --card reads the card's blast-radius, classifies files:
#   - Any app-affecting file (views, platform/api/src, etc) → runs --all
#   - All files non-app (scripts, hooks, skills, rust, configs, docs) → skips smoke
# Override via --all if card is tagged type:swat or impact:wide.
CARD_TAGS=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view ${CARD_ID} 2>/dev/null | grep -E "Domains:" | head -1)
if echo "$CARD_TAGS" | grep -qE "type:swat|impact:wide"; then
  bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/smoke-check.sh --all
else
  bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/smoke-check.sh --card=${CARD_ID}
fi
```

**Gate logic:**
- If `smoke-check.sh` exits non-zero → **STOP.** Print the failures. Builder fixes before re-running `/demo`.
- If the card is tagged `[swat]` → **exempt** from automated smoke check (crisis cards may intentionally break non-critical pages).
- If the card is non-code (docs, decisions, process) → skip `smoke-check.sh`, verify the artifact exists instead.

**After the automated gate passes**, walk the happy path specific to this card:

1. **What changed?** — Read the card description and recent comments for what was built
2. **Is it deployed?** — For code changes: check if the latest commit is running
   ```bash
   cd /Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site && git log --oneline -1
   ```
3. **Does the card-specific happy path work?** — Based on the AC, verify the specific feature:
   - If AC mentions an API endpoint: `curl -s` it
   - If AC mentions a page: check it responds with 200 and shows expected content
   - If AC mentions a script: verify it runs
4. **Report smoke check result:** PASS or FAIL with specifics

If card-specific check FAILS: stop here. Report what broke. Builder fixes before re-running `/demo`.

## Step 4: Stakes brief (HARD GATE)

**Do NOT** lead with what you built or how it works. Lead with **why it matters and what you want Jeff to see.**

**You must print the structured brief below before proceeding. If you skip it or lead with mechanics, the demo is invalid.**

Print:

```
## Demo: #<card-id> — <title>

**Builder:** <owner>
**Gates:** product PASS | code PASS | quality PASS | arch PASS | ops PASS
**Smoke check:** PASS (N pass, 0 fail)
**Why this matters:** <1-2 sentences — what was broken/missing before, what's different now for the user>
**What I want to show you:** <the specific moment or interaction Jeff should watch>
```

**Gate check:** The brief must contain a "Why this matters" line that describes user impact, not implementation. If the brief leads with mechanics ("I built...", "The API now...", "Here's what changed...") → rewrite before proceeding.

**Anti-patterns (FAIL the gate if any appear in the brief):**
- "I built a function that..." — mechanics first, no stakes
- "Here's what changed in the codebase..." — implementation, not impact
- "The API now returns..." — plumbing, not value

**Good examples:**
- "Before this, a role could start building without knowing what files they'd touch. Now the board tells them before they write a line of code. Let me show you on a real card."
- "Notes and music were islands — no connection. The engine found 2,600 links between them. Watch the knowledge graph when I toggle inferred edges."

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log demo.stakes.completed <your-role> card=${CARD_ID}
```

## Step 5: Signal (HARD GATE)

**All four signal actions must complete before showing Jeff. If any fail, stop and fix.**

```bash
# 1. Log demo started on board
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards demo ${CARD_ID}

# 2. Emit spine event
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log card.demo.started <your-role> card=${CARD_ID}
```

**Post the demo brief to Bridge so Jeff sees it without switching windows:**
```bash
# Post stakes summary to Bridge — Jeff sees this on the center panel
curl -s -X POST http://localhost:3470/api/message \
  -H 'Content-Type: application/json' \
  -d "{\"from\": \"<your-role>\", \"text\": \"[demo] #${CARD_ID} — <title>\\n\\n**Why this matters:** <stakes summary>\\n**What to show Jeff:** <specific moment>\"}" \
  > /dev/null 2>&1
```

**Nudge the other roles into demo observer mode** — call MCP per role (skip yourself). Every nudge formally requests ack; recipients MUST reply with substance, not silence. Sender re-nudges until ack received.

```
For each role in [wren, silas, kade] except <your-role>:
  mcp__chorus-api__chorus_nudge_message({
    to: "<role>",
    message: "[demo] #${CARD_ID} — <title>. /gemba <your-role>, card #${CARD_ID}. ACK REQUIRED: confirm observation started within 5 min or reply blocked-on-X."
  })
```

Then **auto-nudge for feedback** — don't wait for Jeff to ask.

**Frame the nudge as concrete consumer-impact questions, not agent-curiosity edge cases.** Edge-case questions ("is this a Quality-horizontal-family-fit?", "is the sibling-script pattern worth naming as a convention?") get edge-case engagement — agents climb into the frame and produce structured 1-2-3 answers that feel substantive but never ask whether the work matters. Concrete consumer questions force the recipient to ground their answer in real users and real impact.

```
For each role in [wren, silas, kade] except <your-role>:
  mcp__chorus-api__chorus_nudge_message({
    to: "<role>",
    message: "[feedback] #${CARD_ID} — <1-line what changed>.
              (1) How does this impact your products?
              (2) How does it impact your users?
              (3) Am I over-building or under-planning?
              ACK REQUIRED: substantive reply within 10 min or blocked-on-X."
  })
```

**The three questions are the only ones to send.** Don't tailor per-role with cleverness; don't substitute agent-curiosity edge cases. The same three force every role to ground in their own consumers, name the value-or-cost, and call out scope mistakes.

**Ack discipline (every nudge, every time)**:
- Sender requests ack explicitly in the nudge body. Format: "ACK REQUIRED: <expected reply shape> within <window> or blocked-on-X."
- Recipient MUST reply with substance OR with `blocked-on-X` (specific blocker named). Silent ignore is contract violation.
- Sender's session tracks outstanding acks; re-nudges at the deadline if no reply lands.
- After 2 re-nudges with no reply, escalates to Jeff with the recipient + card + question.
- Jeff should never have to ask "did X reply?" — the substrate chases, not him. Structural enforcement filed as #2872; until that ships, the discipline lives here.

**Nudge framing anti-patterns (NEVER use these):**
- "Jeff wants your review" — invokes authority, produces rubber stamps
- "ready for acceptance" — pre-frames the expected outcome
- "review and accept" — tells them the answer before they look
- "Jeff is waiting" — creates urgency that shortcuts thinking
- "Is the X pattern worth naming?" / "Does this read as Y-family-fit?" / "Is the AC framing the template for future Z?" — agent-curiosity edge cases. Recipient climbs into the frame and produces 1-2-3 structure that displaces the substantive consumer question.

### Demo Observer Mode (for nudge recipients)

When you receive a `[demo]` nudge, **enter `/gemba <builder-role>` immediately.** Demo observation IS gemba — same tail, same digest loop, same commentary. The nudge is just the trigger.

The `/gemba` skill owns the full observe-loop pattern. Don't duplicate it here.

### Feedback Response (MANDATORY for `[feedback]` nudge recipients)

When you receive a `[feedback]` nudge, **reply with substance.** Share your actual perspective — questions, ideas, concerns, or connections you see. This is not a checkbox exercise.

1. **Check your domain** — look at your work, run relevant checks, think about implications from your expertise
2. **Reply with your real take** — what you noticed, what questions it raises, how it connects to your work. If it's clean, say so briefly and add anything useful. If there's an issue, name it.
3. **Do NOT** just say "got it", "acknowledged", or "LGTM" — that's not feedback, it's noise
4. **Do NOT** mirror the sender's framing — form your own opinion before responding. "AC looks met" without checking is performative, not substantive.

The builder is blocked on acceptance until team feedback lands. Silence forces Jeff to chase replies — that's failure demand.

### 4. Update demo brief with smoke check + stakes (MANDATORY — #1670, #2068)

**For single-card demos (#2090):** No brief file to update. The card comment from Step 1.5 and the spine events are the provenance record.

**For multi-card/pipeline demos:** Append smoke check + stakes to the consolidated brief created in Step 1.5.

**Signal gate check — all 3 must have completed:**
1. Board demo logged (cards demo)
2. Spine event emitted (card.demo.started)
3. Roles nudged (demo observer + feedback)

If any action failed → **STOP.** Fix before proceeding to Step 6.

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log demo.signal.completed <your-role> card=${CARD_ID}
```

## Step 6: Show, don't tell — then wait

**The builder shows the feature working. Not describes it. Not summarizes it.**

1. **Open the page / hit the endpoint / run the command** — Jeff should see the thing, not hear about it
2. **Pause.** Wait for Jeff to react. Do not pre-answer anticipated questions. Do not explain what he's looking at unless he asks.
3. **If Jeff asks to see something specific** (e.g., "show me on the KG page"), go there immediately. No "let me first explain..." — follow his eye.
4. **If Jeff gives feedback**, adapt in real time. The demo is a conversation, not a presentation.
5. **Capture what Jeff saw and how he reacted** — this goes in the brief to Wren. Not just "what was built" but "Jeff asked to see X, noticed Y, wants Z."

**The pattern:** Stakes → Show → Pause → Follow Jeff's attention → Capture reaction.

6. **Scroll for Jeff** — for long pages, use `demo-scroll.sh` to walk through content hands-free:
   ```bash
   bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/demo-scroll.sh down 3   # scroll down 3 sections
   bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/demo-scroll.sh top       # back to top
   bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/demo-scroll.sh bottom    # jump to bottom
   ```

**Anti-patterns:**
- Dumping a summary and asking "thoughts?" — that's a status report, not a demo
- Explaining what's on screen before Jeff has processed it — let him look first
- Answering a question Jeff hasn't asked yet — wait for his actual concern

## Step 7: Accept or reject

**Only Wren or Jeff can accept.** The builder CANNOT mark their own code card Done.

- **Accept:** `cards done <card-id>` + emit `card.accepted` spine event
- **Reject:** `cards reject <card-id> "reason"` + emit `card.rejected` spine event. Builder gets the reason and iterates.

## Rules

- No self-service Done for code changes (DEC-048)
- Smoke check before demo — always (DEC-050)
- Jeff sees every demo — no pre-screening by Wren
- If smoke check fails, the demo doesn't happen
- Keep the prep summary tight — Jeff doesn't want a wall of text
