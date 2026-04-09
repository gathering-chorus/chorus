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
2. Run Steps 1-4 **for each card** — validate, smoke check, prep brief, signal
3. Consolidate into **one demo brief** with a section per card
4. Nudge roles **once** with the full list, not per-card: `[demo] #1459 #1461 #1480 — <pipeline name or summary>`
5. Nudge for feedback **once**: `[feedback] #1459 #1461 #1480 — <summary>. Questions? Concerns?`

## Step 1: Validate card exists and is in WIP or Now (or Done for post-acceptance demos)

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/board-ts view ${CARD_ID}
```

- Card must exist and be in WIP or Now status
- Read the card's AC (acceptance criteria) from description
- Identify the owner (builder)

If card is not in WIP/Now: "Card #${CARD_ID} is in <status> — must be in WIP to demo."

## Step 1.5: Quality gate — AC review (HARD GATE, #1717)

**Before the smoke check, verify the work covers the AC.** This is the gate that catches "done but not done" before Jeff ever sees it.

1. Extract AC items from the card description (from Step 1 output)
2. Get the recent git diff:
   ```bash
   cd /Users/jeffbridwell/CascadeProjects/chorus && git diff HEAD~5 --stat
   ```
3. Run the agent review:
   ```bash
   echo "<review prompt>" | claude -p \
     --model haiku \
     --permission-mode dontAsk \
     --no-session-persistence \
     --max-budget-usd 0.10 \
     --output-format json \
     --disallowedTools "Bash,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task"
   ```
   
   The review prompt includes the card AC and the diff stat. The agent returns:
   ```json
   {"pass": true/false, "gaps": ["description of each gap"]}
   ```

4. **If `pass: false`** → **STOP.** Print the gaps. Builder fixes before re-running `/demo`:
   ```
   Quality gate FAILED for #<card-id>:
   - <gap 1>
   - <gap 2>
   Fix these AC gaps before demo.
   ```

5. **If `pass: true`** → continue to smoke check. Print:
   ```
   Quality gate: PASS — AC coverage verified by agent review.
   ```

6. **If agent fails** (timeout, spawn error, budget exceeded) → **warn but don't block**:
   ```
   Quality gate: SKIPPED — agent review failed (<reason>). Proceeding with smoke check.
   ```

7. Emit spine event:
   ```bash
   /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log quality.gate.completed <your-role> card=${CARD_ID} result=pass|fail|skipped
   ```

**Skip conditions:** Cards tagged `type:chore` or `type:swat` skip the agent review (same as demo gate skip in #1881).

## Step 2: Smoke check (HARD GATE)

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

## Step 3: Open with stakes, not mechanics

**Do NOT** lead with what you built or how it works. Lead with **why it matters and what you want Jeff to see.**

Print a structured demo brief:

```
## Demo: #<card-id> — <title>

**Builder:** <owner>
**Smoke check:** PASS/FAIL
**Why this matters:** <1-2 sentences — what was broken/missing before, what's different now for the user>
**What I want to show you:** <the specific moment or interaction Jeff should watch>
```

**Anti-patterns:**
- "I built a function that..." — mechanics first, no stakes
- "Here's what changed in the codebase..." — implementation, not impact
- "The API now returns..." — plumbing, not value

**Good examples:**
- "Before this, a role could start building without knowing what files they'd touch. Now the board tells them before they write a line of code. Let me show you on a real card."
- "Notes and music were islands — no connection. The engine found 2,600 links between them. Watch the knowledge graph when I toggle inferred edges."

## Step 4: Signal

```bash
# Log demo started
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/board-ts demo ${CARD_ID}

# Emit spine event
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

### Auto-generate demo brief (MANDATORY — #1670)

The builder does NOT manually write this brief. The demo skill generates it mechanically from card data. This is the provenance record — it proves a demo happened.

**Generate the brief automatically:**

1. Parse AC items from the card description (lines starting with `- [ ]` or `- [x]`)
2. Count checked vs total
3. Write the brief — no manual fill-in, no placeholders

```bash
# Extract AC from card description
AC_LINES=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/board-ts view ${CARD_ID} | grep -E '^\s*- \[[ x]\]')
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

**After the demo completes (Step 5)**, append to the existing brief:
```bash
cat >> /Users/jeffbridwell/CascadeProjects/chorus/roles/wren/briefs/$(date +%Y-%m-%d)-demo-${CARD_ID}.md << EOF

## Demo outcome
- **What Jeff saw:** <what was shown>
- **Jeff's reaction:** <questions asked, feedback given, direction changed>
- **Outcome:** accepted / rejected / iterate on X
EOF
```

## Step 5: Show, don't tell — then wait

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

## Step 6: Accept or reject

**Only Wren or Jeff can accept.** The builder CANNOT mark their own code card Done.

- **Accept:** `board-ts done <card-id>` + emit `card.accepted` spine event
- **Reject:** `board-ts reject <card-id> "reason"` + emit `card.rejected` spine event. Builder gets the reason and iterates.

## Rules

- No self-service Done for code changes (DEC-048)
- Smoke check before demo — always (DEC-050)
- Jeff sees every demo — no pre-screening by Wren
- If smoke check fails, the demo doesn't happen
- Keep the prep summary tight — Jeff doesn't want a wall of text
