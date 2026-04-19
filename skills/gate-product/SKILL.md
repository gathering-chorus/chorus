---
name: gate-product
description: Product gate — verify AC complete, demo evidence, domain visibility, spine contract. Wren only.
user-invocable: true
---

# /gate-product — Product Gate

Fires before handoff to engineering. Verifies the work is product-complete — tracked, described, discoverable. **Wren only.**

Everything we build must be visible in the system. No dark features. No untracked services. No stale domain graph.

## Arguments

```
/gate-product <card-id>
```

## Owner Check

Only Wren can run this gate. If another role invokes it:
```
Product gate is owned by Wren.
```
Exit — no checks run.

## Applicability Check

Read the card with `cards view <card-id>`. Check the card type label.

- Board-process / chore cards with no user-visible change: **SKIP** — "Product gate not applicable for this card type."
- All cards with user-visible changes, new features, fixes, enhancements: **RUN**.

If skipped, emit: `gate.product.skipped` spine event. Exit.

## Automated Checks (run all, collect results)

### 1. AC complete

```bash
CARD_VIEW=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view ${CARD_ID} 2>&1)

# Count total AC items and checked items
TOTAL_AC=$(echo "$CARD_VIEW" | grep -cE '^\s*- \[[ x]\]')
CHECKED_AC=$(echo "$CARD_VIEW" | grep -cE '^\s*- \[x\]')

if [ "$TOTAL_AC" -eq 0 ]; then
  echo "FAIL: no AC items found on card"
elif [ "$CHECKED_AC" -lt "$TOTAL_AC" ]; then
  echo "FAIL: $CHECKED_AC/$TOTAL_AC AC items checked"
  echo "$CARD_VIEW" | grep -E '^\s*- \[ \]'
else
  echo "PASS: $CHECKED_AC/$TOTAL_AC AC items checked"
fi
```

**Pass:** All AC checkboxes are `[x]`.
**Fail:** Any unchecked `[ ]` items — list them.

### 2. Demo evidence

```bash
# Check for demo evidence in Comments section only (not description body)
DEMO_COMMENT=$(echo "$CARD_VIEW" | sed -n '/^  Comments/,$ p' | grep -c 'demo:preflight-pass')

if [ "$DEMO_COMMENT" -gt 0 ]; then
  echo "PASS: demo evidence found — card comment demo:preflight-pass"
else
  echo "FAIL: no demo evidence for #${CARD_ID} — add 'demo:preflight-pass' comment to card after demo"
fi
```

**Pass:** Card has a `demo:preflight-pass` comment.
**Fail:** No comment found. Run the demo, then add `demo:preflight-pass` as a card comment before re-running gate.

### 3. Description fidelity

```bash
# Check card description has Experience section and is non-trivial
EXPERIENCE=$(echo "$CARD_VIEW" | sed -n '/## Experience/,/## /p' | head -10)
DESC_WORDS=$(echo "$CARD_VIEW" | sed -n '/Desc:/,/Domains:/p' | wc -w | tr -d ' ')

if [ -z "$EXPERIENCE" ]; then
  echo "FAIL: no Experience section in card description"
elif [ "$DESC_WORDS" -lt 20 ]; then
  echo "WARN: description is thin ($DESC_WORDS words) — may not match what shipped"
else
  echo "PASS: description has Experience section ($DESC_WORDS words)"
fi
```

**Pass:** Experience section exists, description is substantive.
**Fail:** Missing Experience section.
**Warn:** Description under 20 words — review for staleness.

### 4. Domain registration

```bash
# Extract domain from card labels
DOMAIN=$(echo "$CARD_VIEW" | grep -oE 'domain:\w+' | head -1 | sed 's/domain://')

if [ -n "$DOMAIN" ]; then
  # Query Athena crawler API — does this domain exist and have data?
  CRAWLER_RESPONSE=$(curl -s "http://localhost:3340/api/chorus/domain/${DOMAIN}" 2>/dev/null)
  if echo "$CRAWLER_RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); sys.exit(0 if d.get('domain') else 1)" 2>/dev/null; then
    echo "PASS: domain '${DOMAIN}' registered in Athena"
  else
    echo "WARN: domain '${DOMAIN}' not found in Athena crawler — may need graph population"
  fi
else
  echo "WARN: no domain label on card — can't verify registration"
fi
```

**Pass:** Card's domain exists in the Athena domain graph.
**Warn:** Domain not found or no domain label — flag for review.

### 5. Spine contract

```bash
# Check if changed files contain chorus-log calls
cd /Users/jeffbridwell/CascadeProjects/chorus
CHANGED=$(git diff HEAD~3 --name-only 2>/dev/null)

if [ -n "$CHANGED" ]; then
  SPINE_CALLS=$(echo "$CHANGED" | xargs grep -l 'chorus-log\|chorus_log' 2>/dev/null)
  if [ -n "$SPINE_CALLS" ]; then
    CALL_COUNT=$(echo "$CHANGED" | xargs grep -c 'chorus-log\|chorus_log' 2>/dev/null | awk -F: '{s+=$2} END {print s}')
    echo "PASS: ${CALL_COUNT} spine event call(s) in changed files"
  else
    # Not all cards need spine events — skills and scripts do, docs don't
    CARD_TYPE=$(echo "$CARD_VIEW" | grep -oE 'type:\w+' | head -1 | sed 's/type://')
    if [ "$CARD_TYPE" = "new" ] || [ "$CARD_TYPE" = "enhance" ]; then
      echo "WARN: no spine event calls in changed files — new/enhance cards should emit events"
    else
      echo "PASS: no spine calls expected for type:${CARD_TYPE}"
    fi
  fi
else
  echo "WARN: no changed files detected"
fi
```

**Pass:** Changed files contain spine event calls, or card type doesn't require them.
**Warn:** New/enhance cards with no spine events — likely missing observability.

## Manual Confirms (shown after automated checks)

Only shown if no automated checks FAIL (WARN is ok).

### 6. Service design coverage

Print the question:
```
Does this card introduce or change a service boundary?
If yes — does a service design exist for it? (check cards sequence:athena)
```

**Pass:** No new service boundary, or service design exists.
**Fail:** New service with no design — card the gap.

### 7. Product story impact

Print the question:
```
Does this change how we'd describe the product to Jeff?
If yes — flag which doc needs updating (PRODUCT_VISION.md, about pages, domain description).
```

**Pass:** No product story change, or doc update flagged.
**Fail:** Story changed but no doc update planned — card it.

## Result

Print summary:

```
## /gate-product #<card-id>

  AC complete:          PASS | FAIL (N/M checked)
  Demo evidence:        PASS | FAIL (no brief)
  Description fidelity: PASS | FAIL | WARN
  Domain registration:  PASS | WARN (not in Athena)
  Spine contract:       PASS | WARN (no events)
  Service design:       PASS | FAIL (manual)
  Product story:        PASS | FAIL (manual)

  VERDICT: PASS | FAIL
```

VERDICT is FAIL if any automated check is FAIL or any manual confirm is FAIL.
VERDICT is PASS if all checks are PASS or WARN. WARNs are logged but don't block.

## On Pass

1. Emit spine event: `gate.product.passed` with card ID
2. Add card comment: "gate:product-pass — Wren"
3. Nudge Kade: "gate:product passed on #<card-id> — run /gate-code"

## On Fail

1. Emit spine event: `gate.product.failed` with card ID and failing items
2. Print failing items. Fix before re-running. No forward nudge.
