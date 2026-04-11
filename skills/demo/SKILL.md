---
name: demo
description: Proving gate — smoke check, prep summary, signal to Wren, builder cannot self-accept.
user-invocable: true
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
2. Run Steps 1-5 **for each card** — validate, gates, smoke check, stakes brief, signal
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

## Step 2: Gate chain (HARD GATE)

**Before the smoke check, run the full gate chain.** This is how the team verifies quality together — every demo, every time.

**Skip conditions:** Cards tagged `type:chore` or `type:swat` skip the gate chain (crisis/housekeeping cards).

### Gate sequence

1. **Run `/gate-product <card-id>`** — AC described, experience section, domain registered, spine contract. Wren runs this. On pass, nudges Kade for gate-code. **Note:** The "demo evidence" check (check 2) is expected to WARN when running inside `/demo`, since the demo brief is created later in Step 5. This is normal — the check validates that a brief exists for re-demos, not the first pass.
2. **Run `/gate-code <card-id>`** — tests green, build clean, no new warnings, pattern match. Kade runs this.
3. **Run `/gate-quality <card-id>`** — hooks pass, regression clean, no console.log, debt check. Kade runs this. On pass, auto-nudges Silas for arch review.
4. **Wait for `/gate-arch` pass from Silas** — system fit, namespace conventions, domain boundaries. Silas runs this after receiving the nudge.

Check for gate results via card comments:
```bash
# Check which gates have passed
CARD_COMMENTS=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards comments ${CARD_ID} 2>/dev/null)
PRODUCT_PASS=$(echo "$CARD_COMMENTS" | grep -c 'gate:product-pass')
CODE_PASS=$(echo "$CARD_COMMENTS" | grep -c 'gate:code-pass')
QUALITY_PASS=$(echo "$CARD_COMMENTS" | grep -c 'gate:quality-pass')
ARCH_PASS=$(echo "$CARD_COMMENTS" | grep -c 'gate:arch-pass')
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
   ```

2. **Run only gates you own** — if a gate is missing and you own it, run it inline. If you don't own it, nudge the owner:
   - `gate:product` missing → Wren runs `/gate-product`, or nudge Wren
   - `gate:code` missing → Kade runs `/gate-code`, or nudge Kade
   - `gate:quality` missing → Kade runs `/gate-quality`, or nudge Kade
   - `gate:arch` missing → Silas runs `/gate-arch`, or nudge Silas

3. **If gates are missing and you've nudged:** Report which gates are pending and who was nudged. **Do not block silently** — tell Jeff what you're waiting on:
   ```
   Gate chain waiting for #<card-id>:
     gate:product  PASS
     gate:code     PASS
     gate:quality  MISSING — nudged Kade
     gate:arch     PASS
   Waiting on Kade for gate:quality. Will proceed when comment appears.
   ```

- **If all four gates passed:** Print gate summary and continue to Step 3:
  ```
  Gate chain: PASS
    gate:product  PASS — Wren
    gate:code     PASS — Kade
    gate:quality  PASS — Kade
    gate:arch     PASS — Silas
  ```
- **If any gate FAILED:** STOP. Print the failing gate's output. Builder fixes before re-running `/demo`.

## Step 3: Smoke check (HARD GATE)

**Run the automated smoke check first.** This is a gate — non-zero exit blocks the demo.

```bash
# For code cards that touch app pages: run smoke-check.sh
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/smoke-check.sh --all
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
**Gates:** code PASS | quality PASS | arch PASS
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

**Nudge the other roles into demo observer mode:**
```bash
# Nudge both other roles (skip yourself)
for ROLE in wren silas kade; do
  [ "$ROLE" = "<your-role>" ] && continue
  bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge "$ROLE" \
    "[demo] #${CARD_ID} — <title>. /gemba <your-role>, card #${CARD_ID}." 2>/dev/null || true
done
```

Then **auto-nudge for feedback** — don't wait for Jeff to ask.

**Frame the nudge in the recipient's domain, not Jeff's authority.** The way you frame the nudge shapes the response. If you say "Jeff wants your review," they rubber-stamp. If you say "this touches your domain — what does it mean for X?", they actually look.

```bash
# Ask each role to verify from THEIR perspective — frame in their domain
for ROLE in wren silas kade; do
  [ "$ROLE" = "<your-role>" ] && continue
  bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge "$ROLE" \
    "[feedback] #${CARD_ID} — <1-line what changed>. <specific question for this role's domain>." 2>/dev/null || true
done
```

**How to frame per role:**
- **To Wren (PM):** Frame as product impact — "How does this change the user experience? Does the AC match what Jeff described?"
- **To Silas (Architect):** Frame as infrastructure/ops — "Any deployment concerns? Does this interact with health checks or gates you own?"
- **To Kade (Engineer):** Frame as code quality — "Does the implementation approach make sense? Any test gaps you see?"

**Nudge framing anti-patterns (NEVER use these):**
- "Jeff wants your review" — invokes authority, produces rubber stamps
- "ready for acceptance" — pre-frames the expected outcome
- "review and accept" — tells them the answer before they look
- "Jeff is waiting" — creates urgency that shortcuts thinking

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

### 4. Auto-generate demo brief (MANDATORY — #1670)

The builder does NOT manually write this brief. The demo skill generates it mechanically from card data. This is the provenance record — it proves a demo happened.

**Generate the brief automatically:**

1. Parse AC items from the card description (lines starting with `- [ ]` or `- [x]`)
2. Count checked vs total
3. Write the brief — no manual fill-in, no placeholders

```bash
# Extract AC from card description
AC_LINES=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view ${CARD_ID} | grep -E '^\s*- \[[ x]\]')
AC_TOTAL=$(echo "$AC_LINES" | grep -c '^\s*- \[' 2>/dev/null | tr -d '\n')
AC_CHECKED=$(echo "$AC_LINES" | grep -c '^\s*- \[x\]' 2>/dev/null | tr -d '\n')
: "${AC_TOTAL:=0}" "${AC_CHECKED:=0}"

cat > /Users/jeffbridwell/CascadeProjects/chorus/roles/wren/briefs/$(date +%Y-%m-%d)-demo-${CARD_ID}.md << EOF
# Demo ready: #${CARD_ID} — <title from board-ts>

**Builder:** <owner from board-ts>
**Smoke check:** PASS (<N> pass, 0 fail)
**Demo initiated by:** <your-role>
**Timestamp:** $(TZ=America/New_York date '+%Y-%m-%dT%H:%M')

## AC Status (${AC_CHECKED}/${AC_TOTAL})
${AC_LINES}

## What to show Jeff
<1-2 lines from your Step 3 stakes summary>

Auto-generated by /demo skill.
Ready for Jeff + Wren review.
EOF
```

**Signal gate check — all 4 must have completed:**
1. Board demo logged (cards demo)
2. Spine event emitted (card.demo.started)
3. Roles nudged (demo observer + feedback)
4. Demo brief written to Wren's briefs/

If any action failed → **STOP.** Fix before proceeding to Step 6.

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log demo.signal.completed <your-role> card=${CARD_ID}
```

**After the demo completes (Step 6)**, append to the existing brief:
```bash
cat >> /Users/jeffbridwell/CascadeProjects/chorus/roles/wren/briefs/$(date +%Y-%m-%d)-demo-${CARD_ID}.md << EOF

## Demo outcome
- **What Jeff saw:** <what was shown>
- **Jeff's reaction:** <questions asked, feedback given, direction changed>
- **Outcome:** accepted / rejected / iterate on X
EOF
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

- **Accept:** `board-ts done <card-id>` + emit `card.accepted` spine event
- **Reject:** `board-ts reject <card-id> "reason"` + emit `card.rejected` spine event. Builder gets the reason and iterates.

## Rules

- No self-service Done for code changes (DEC-048)
- Smoke check before demo — always (DEC-050)
- Jeff sees every demo — no pre-screening by Wren
- If smoke check fails, the demo doesn't happen
- Keep the prep summary tight — Jeff doesn't want a wall of text
