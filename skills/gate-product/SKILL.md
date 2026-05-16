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

**Source of truth (#2940 Move 0):** `data/athena/tree.json` is canonical. Verify the card's `domain:<slug>` label resolves to a `chorus:domain-<slug>` IRI in the tree. After Move 1 ships SHACL, the same check runs against the graph; for now JSON is the answer.

```bash
# Extract domain slug from card labels
DOMAIN=$(echo "$CARD_VIEW" | grep -oE 'domain:\w+' | head -1 | sed 's/domain://')

if [ -n "$DOMAIN" ]; then
  # Move 0 path — check tree.json for chorus:domain-<slug> OR chorus:<slug>
  # (Products are bare `chorus:<slug>`; Domains are `chorus:domain-<slug>`).
  TREE_JSON=$(curl -s http://localhost:3340/api/athena/tree 2>/dev/null)
  if [ -z "$TREE_JSON" ]; then
    # Fallback: read from disk in canonical path
    TREE_JSON=$(cat "${CHORUS_ROOT:-$HOME/CascadeProjects/chorus}/data/athena/tree.json" 2>/dev/null)
  fi
  if [ -n "$TREE_JSON" ]; then
    DOMAIN_HIT=$(echo "$TREE_JSON" | python3 -c "
import json,sys
d=json.load(sys.stdin)
slug='$DOMAIN'
candidates={f'chorus:domain-{slug}', f'chorus:{slug}'}
hit = any(p['iri'] in candidates for p in d.get('products',[]))
hit = hit or any(dm['iri'] in candidates for dm in d.get('domains',[]))
hit = hit or any(s['iri'] in candidates for s in d.get('services',[]))
print('yes' if hit else 'no')
" 2>/dev/null)
    if [ "$DOMAIN_HIT" = "yes" ]; then
      echo "PASS: domain '${DOMAIN}' resolves to a tree.json IRI"
    else
      echo "FAIL: domain '${DOMAIN}' does NOT exist in data/athena/tree.json — add it to the tree before claiming this domain on a card, or correct the card label"
    fi
  else
    echo "WARN: tree.json unavailable (api + disk both failed) — can't verify domain"
  fi
else
  echo "WARN: no domain label on card — can't verify registration"
fi
```

**Pass:** Card's domain resolves to a tree.json IRI.
**Fail:** Domain claimed but not in tree — refuse the gate. Closes the "I named a domain that isn't a domain" failure mode (#2940).
**Warn:** No domain label OR tree.json unreachable.

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

1. **Emit `probe.evidence` spine event** with a `probe_type` field naming what KIND of evidence the PASS rests on. **No probe = no PASS.** This is the structural enforcement of the rule "every gate-PASS must trace to a probe, not a paper trail" (Jeff 2026-04-30, after the #2625 paper-trail PASS cost a team-blocked window). The `probe.evidence` event makes the probe auditable — the spine-emit-drift bats catches gate-PASS comments without correlated probe.evidence.

   **Probe types (per Silas arch-lens 2026-04-30):**
   - `live-system` — ran a live probe against a running service/endpoint (curl, db query, log tail). Evidence = stdout/artifact.
   - `diff-read` — read the diff at specific line ranges and verified each AC item matches. Evidence = `file:line` references mapping AC items to diff sections.
   - `doc-check` — verified text edits to docs/skills/markdown matched what AC promised. Evidence = `file:line` references + change-summary.
   - `grep-zero-hits` — verified absence of a retired symbol/route. Evidence = grep command + zero-hits output.
   - `bats-suite` — ran bats files and captured pass count. Evidence = bats output summary.

   Pick the type that actually applies. Do NOT emit a `live-system` probe when the card is doc-only — that recreates the performative-gate problem in a new shape. The honest probe matters more than the impressive one.
2. Emit spine event: `gate.product.passed` with card ID
3. Add card comment: "gate:product-pass — Wren" — include a one-line summary of the probe and what it returned
4. Nudge Kade: "gate:product passed on #<card-id> — run /gate-code"

## On Fail

1. Emit spine event: `gate.product.failed` with card ID and failing items
2. Print failing items. Fix before re-running. No forward nudge.
