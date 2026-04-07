---
name: pull
description: Pull a card to WIP — enforce gates, declare state, start building.
user-invocable: true
---

# /pull — Pull Card to WIP and Start Building

`/pull <card-id>` is a **go signal**. Pull the card, set up state, then start building immediately. No checkpoint, no pause for approval.

**Any role can invoke this.** Jeff can assign work with `/pull 1092 kade`. A builder calls `/pull 1092` for themselves.

## Arguments

```
CARD_ID=<first argument>
ROLE_OVERRIDE=<optional second argument — target role, defaults to invoking role>
```

If no card ID given, check the role's Next cards and suggest the highest-priority smallest card (DEC-049 WSJF).

## Step 1: Validate card exists and is pullable

```bash
bash /Users/jeffbridwell/CascadeProjects/platform/scripts/board-ts view ${CARD_ID}
```

- Card must exist and be in **Next** or **Later** status
- If card is already in WIP: "Card #${CARD_ID} is already in WIP — owned by <owner>."
- If card is in Done: "Card #${CARD_ID} is already Done."
- Identify the owner from the card labels

## Step 2: Pre-flight quality check (MANDATORY)

Before attempting `board-ts move`, verify the card has everything it needs. **Fix what you can, flag what you can't. Never hit the gate error blind.**

From the `board-ts view` output in Step 1, check:

1. **Experience section exists?** (#1839) — Description must contain `## Experience` with 2-5 sentences in Jeff's voice describing what he sees/feels/gets.
   - If missing: **draft Experience from the title and AC.** Write it as Jeff would say it — his words, his sequence. Update with `board-ts set <id> desc="..."` before moving. Route to Wren if unsure.

2. **AC exists?** — Description must contain acceptance criteria (look for "## AC", "Acceptance Criteria", or bullet points with testable conditions).
   - If missing: **draft AC from the Experience section and title.** AC derives from the experience, not the other way around. Update the card with `board-ts set <id> desc="..."` before moving. Don't ask Jeff — draft it yourself.

3. **Chunk tagged?** — Card must have a `chunk:` label.
   - If missing: infer from domain labels or file paths. `domain:photos` → `chunk:memory`, `domain:documents` → `chunk:app`, etc. Tag with `board-ts set <id> chunk=<chunk>`.

4. **Sequence tagged?** — Card should have a sequence label. Warn if missing but don't block.

5. **Files listed?** — Description should reference specific files for blast radius.
   - If missing and you know the files from the AC: add them. If you don't know: proceed, blast radius will auto-generate on move.

**If AC was missing and you drafted it:** tell Jeff what you wrote so he can correct it. One line: "Drafted AC for #<id> — <summary>. Override if wrong."

**Anti-pattern:** Attempting `board-ts move` and discovering the gate error. That's the old pattern. The pre-flight catches it first.

## Step 3: WIP limit check (DEC-051)

```bash
bash /Users/jeffbridwell/CascadeProjects/platform/scripts/board-ts mine <role> | grep -c WIP
```

- WIP limit is 3 total, 1 per role is healthy
- If the target role already has 2+ WIP cards, **warn**: "⚠ <role> has <N> WIP cards. Finish or park one first?"
- If total WIP is 3+, **warn**: "⚠ Team WIP at <N>/3. Consider finishing before pulling."
- Warnings don't block — Jeff can override. But surface the cost.

## Step 4: Move to WIP

```bash
bash /Users/jeffbridwell/CascadeProjects/platform/scripts/board-ts move ${CARD_ID} WIP
```

This automatically generates a blast radius comment on the card (DEC-072) via the board SDK — no manual analysis needed.

## Step 5: Declare state

```bash
# Extract card type from board-ts view output (type:fix, type:new, etc.)
CARD_TYPE=$(bash /Users/jeffbridwell/CascadeProjects/platform/scripts/board-ts view ${CARD_ID} | grep -oE 'type:\w+' | head -1 | sed 's/type://')
bash /Users/jeffbridwell/CascadeProjects/platform/scripts/role-state <role> building card=${CARD_ID} type=${CARD_TYPE:-unknown}
```

## Step 6: Emit signal

```bash
/Users/jeffbridwell/CascadeProjects/platform/scripts/chorus-log card.pulled <role> card=${CARD_ID}
```

## Step 7: Brief (if cross-role)

If the pulling role is NOT the card owner, or if there's AC/context the builder needs:
- Print the card's AC and description
- Note any briefs in the builder's inbox related to this card
- Note any dependency cards that must complete first

## Step 7.5: Domain context injection (MANDATORY)

**Always call the domain service.** Every card pull reads domain state before the builder starts.

```bash
# Extract domain from card labels (from board-ts view output)
DOMAIN=$(echo "$CARD_VIEW" | grep -oE 'domain:\w+' | head -1 | sed 's/domain://')

if [ -n "$DOMAIN" ]; then
  # Query domain service API — returns UI pages, API endpoints, pipelines, data, known issues
  curl -s "http://localhost:3340/api/chorus/domain/${DOMAIN}" | python3 -c "
import json, sys
d = json.load(sys.stdin)
print(f'## Domain: {d.get(\"domain\")} ({d.get(\"product\")}/{d.get(\"step\")})')
print(f'Description: {d.get(\"description\",\"\")}')
sections = d.get('sections', {})
for name, content in sections.items():
    if isinstance(content, dict) and 'table' in content:
        print(f'### {content.get(\"title\", name)}')
        for row in content['table'][:5]:
            print(f'  {\" | \".join(str(c) for c in row)}')
cards = d.get('cards', {})
print(f'### Cards: {cards.get(\"total\",0)} total, {cards.get(\"wip\",0)} WIP')
" 2>/dev/null
fi
```

**Read the domain state before writing any code.** The domain service shows you what exists, what's broken, what's in flight. The ICD pre-read hook will block you if you skip this.

## Step 8: Start building

**Do not stop after the pull summary.** Read the AC, understand the card, and begin work immediately. `/pull` is not a status report — it's the start of execution.

## Output

Brief summary, then get to work:

```
Pulled #<card-id> — <title>. WIP: <role> <N>/1, team <N>/3.
```

Then start building.

## Rules

- Pull = go. Start building immediately after the pull completes.
- WIP warnings don't block — but always surface them
- Blast radius is auto-generated by `board-ts move WIP` — don't duplicate it manually
- Always emit the spine event — this feeds Borg flow metrics
- If Jeff says `/pull 1092 kade`, move it and brief Kade — don't ask for confirmation
