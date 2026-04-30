---
name: pull
description: Pull a card to WIP — enforce gates, declare state, start building.
user-invocable: true
---

# /pull — Pull Card to WIP and Start Building

`/pull <card-id>` is a **go signal** with hard gates. Pull the card, pass the gates, then build immediately. No checkpoint, no pause for approval.

**This is the single engineering entry point.** `/pair`, `/jdi`, and any other build-starting skill delegates to `/pull` for card setup. All engineering gates live here.

**Any role can invoke this.** Jeff can assign work with `/pull 1092 kade`. A builder calls `/pull 1092` for themselves.

## Arguments

```
CARD_ID=<first argument>
ROLE_OVERRIDE=<optional second argument — target role, defaults to invoking role>
```

If no card ID given, check the role's Next cards and suggest the highest-priority smallest card (DEC-049 WSJF).

## Step 1: Validate (HARD GATE)

```bash
CARD_VIEW=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view ${CARD_ID} 2>&1)
```

**All must be true or the pull stops:**
- Card exists
- Card is in **Next** or **Later** status
- Card has AC (acceptance criteria) in description
- Card has Experience section in description

If any check fails → **STOP:**
- Card not found: `Card #${CARD_ID} does not exist.`
- Already WIP: `Card #${CARD_ID} is already in WIP — owned by <owner>.`
- Already Done: `Card #${CARD_ID} is already Done.`
- No AC: `Card #${CARD_ID} has no acceptance criteria. Add AC before pulling.`
- No Experience: `Card #${CARD_ID} has no Experience section. Add Experience before pulling.`

**Fix what you can:** If AC or Experience is missing and you can draft it from the card title/description, draft it, update with `cards update <id> --desc "..."`, and tell Jeff: "Drafted AC for #<id> — <summary>. Override if wrong."

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pull.validate.completed <role> card=${CARD_ID} result=pass|fail
```

## Step 2: Preflight (HARD GATE)

**Card must be tagged and filed before it enters WIP. Fix what you can, flag what you can't.**

1. **Chunk tagged?** — Card must have a `chunk:` label.
   - If missing: infer from domain labels. Tag with `cards update <id> --chunk <chunk>`.
   - If can't infer → **STOP:** `Card #${CARD_ID} needs a chunk label.`

2. **Domain tagged?** — Card must have a `domain:` label.
   - If missing and obvious from title/AC: tag it. If not → **STOP.**

3. **Files listed?** — Description should reference specific files for blast radius.
   - If missing and you know the files from the AC: add them.
   - If missing and unknown: proceed — blast radius auto-generates on move.

4. **Sequence tagged?** — Warn if missing but don't block.

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pull.preflight.completed <role> card=${CARD_ID} result=pass|fail
```

## Step 3: WIP check (HARD GATE)

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards list --status WIP 2>&1
```

- WIP limit is 3 total, 1 per role is healthy
- If the target role already has 2+ WIP cards → **WARN:** `<role> has <N> WIP cards. Finish or park one first?`
- If total WIP is 3+ → **WARN:** `Team WIP at <N>/3. Consider finishing before pulling.`
- Warnings surface the cost but don't block — Jeff can override.

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pull.wip_check.completed <role> card=${CARD_ID} wip_count=<N>
```

## Step 4: Domain context (HARD GATE)

**Read domain state before writing code. No building blind.**

### Fuseki health check (#2033)

```bash
FUSEKI_OK=$(curl -sf --max-time 3 -o /dev/null -w '%{http_code}' "http://localhost:3030/$/ping" 2>/dev/null)
if [ "$FUSEKI_OK" != "200" ]; then
  echo "BLOCKED: Fuseki is down (localhost:3030). Domain context, SHACL validation, and ontology queries all depend on it."
  echo "  Fix: check launchctl list | grep fuseki, then app-state.sh status"
  /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pull.fuseki_gate.failed <role> card=${CARD_ID}
  exit 1
fi
```

```bash
DOMAIN=$(echo "$CARD_VIEW" | grep -oE 'domain:\w+' | head -1 | sed 's/domain://')

if [ -n "$DOMAIN" ]; then
  curl -s "http://localhost:3340/api/chorus/domain/${DOMAIN}" | python3 -c "
import json, sys
d = json.load(sys.stdin)
print(f'## Domain: {d.get(\"domain\")} ({d.get(\"product\",\"\")}/{d.get(\"step\",\"\")})')
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

**Gate check:** Domain service must respond. If the API is down or returns an error, **WARN** but don't block — print: `Domain service unavailable — building without domain context.`

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pull.domain_context.completed <role> card=${CARD_ID} domain=${DOMAIN}
```

### Knowledge injection (#1905)

**After domain context loads, surface governing artifacts.** The role sees relevant docs before writing code — service designs, decisions, ADRs, research.

```bash
if [ -n "$DOMAIN" ]; then
  CATALOG=$(curl -s --max-time 5 "http://localhost:3000/api/doc-catalog" 2>/dev/null)
  if [ -n "$CATALOG" ]; then
    echo "$CATALOG" | python3 -c "
import json, sys
d = json.load(sys.stdin)
domain = '${DOMAIN}'.lower()
groups = d.get('groups', [])
hits = []
for g in groups:
    for doc in g.get('docs', []):
        title = doc.get('title', '').lower()
        href = doc.get('href', '').lower()
        if domain in title or domain in href:
            hits.append(doc)
if hits:
    print(f'## Knowledge: {len(hits)} artifacts for domain \"{domain}\"')
    for h in hits[:10]:
        print(f'  - {h.get(\"title\",\"?\")} ({h.get(\"artifactType\",\"?\")}) → {h.get(\"href\",\"\")}')
    if len(hits) > 10:
        print(f'  ... and {len(hits)-10} more')
" 2>/dev/null
  fi
fi
```

**Not a gate — informational only.** If the doc-catalog is down or returns nothing, proceed silently.

## Step 4.5: Design gate (HARD GATE) — #1396

**Before code starts, the domain must have actors and dependency edges.** This gate checks the completeness API's `lifecycle.wip` stage. If the domain isn't design-ready, the pull is blocked.

**Skip conditions:** Cards tagged `type:chore`, `type:swat`, or `type:fix` skip this gate — they're reactive work that shouldn't wait on domain design.

```bash
CARD_TYPE=$(echo "$CARD_VIEW" | grep -oE 'type:\w+' | head -1 | sed 's/type://')

if [ "$CARD_TYPE" = "new" ] || [ "$CARD_TYPE" = "enhance" ]; then
  # Find the domain's subdomain ID (e.g., domain:chorus → chorus-domain or similar)
  DOMAIN_SD="${DOMAIN}-domain"
  COMP=$(curl -s "http://localhost:3340/api/athena/subdomains/${DOMAIN_SD}/completeness" 2>/dev/null)
  WIP_PASS=$(echo "$COMP" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['data']['lifecycle']['wip']['pass'])" 2>/dev/null)
  WIP_MISSING=$(echo "$COMP" | python3 -c "import json,sys; d=json.load(sys.stdin); print(', '.join(d['data']['lifecycle']['wip']['missing']))" 2>/dev/null)

  if [ "$WIP_PASS" = "False" ]; then
    echo "BLOCKED: Design gate — domain '$DOMAIN' is missing: ${WIP_MISSING}"
    echo "  Populate actors and edges before pulling type:${CARD_TYPE} cards."
    echo "  POST http://localhost:3340/api/athena/subdomains/${DOMAIN_SD}/actors"
    echo "  See: http://localhost:3000/gathering-docs/domain-detail.html?id=${DOMAIN_SD}"
    # Emit spine event with fail
    /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pull.design_gate.completed <role> card=${CARD_ID} domain=${DOMAIN} result=fail missing="${WIP_MISSING}"
    exit 1
  fi
fi
```

**Gate logic:**
- `type:new` or `type:enhance` → check `lifecycle.wip.pass` from completeness API
- If `False` → **STOP.** Print missing sections and the POST endpoint to populate them.
- `type:chore`, `type:swat`, `type:fix` → **exempt**, skip the check
- If completeness API is unreachable → **WARN** but don't block

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pull.design_gate.completed <role> card=${CARD_ID} domain=${DOMAIN} result=pass
```

## Step 5: TDD readiness (HARD GATE)

**Set the expectation for test-first before any code is written.** This gate doesn't block — `tdd_gate.rs` enforces at edit time. But the builder should know what tests to write before they start.

From the AC items, suggest:
1. **Test file path** — based on the card's domain and files listed. E.g., AC mentions `server.ts` → suggest `tests/athena.test.ts`.
2. **Test cases** — one per AC item. Frame as what Jeff sees, not implementation: "When I POST /api/athena/subdomains, a new subdomain appears on the domain page."
3. **Red first** — remind: "Write these tests first. They should fail (red). Then write code to make them pass (green). DEC-1674."

Print:
```
## TDD Readiness

Tests first (DEC-1674). Suggested test structure from AC:

  File: <suggested test file path>
  Cases:
    - <AC item 1 as test case>
    - <AC item 2 as test case>
    ...

Write tests → red → code → green → demo.
```

**Skip conditions:** Cards tagged `type:chore` or `type:swat` skip TDD readiness.

Emit spine event:
```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pull.tdd_readiness.completed <role> card=${CARD_ID}
```

## Step 6: Move + declare + signal

All gates passed. Execute the pull:

```bash
# Move to WIP (auto-generates blast radius comment)
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards move ${CARD_ID} WIP

# Declare state — #2467: card belongs to the board, not role-state.
# role-state owns session/attention only; card_type is queried from
# the board by hooks that need it (types.rs::card_type_for_role).
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/role-state <role> building

# Emit spine event
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log card.pulled <role> card=${CARD_ID}
```

If cross-role pull: print the card's AC, note briefs in the builder's inbox, note dependency cards.

## Step 7: Build

**Do not stop after the pull summary.** Start building immediately.

Print one line:
```
Pulled #<card-id> — <title>. WIP: <role> <N>/1, team <N>/3.
```

Then start building.

## Rules

- Pull = go. Start building immediately after gates pass.
- Gates 1-5 are HARD GATES — each emits a spine event, each must pass before proceeding.
- WIP warnings surface cost but don't block — Jeff can override.
- Blast radius is auto-generated by `cards move WIP` — don't duplicate manually.
- Always emit spine events — they feed Borg flow metrics.
- If Jeff says `/pull 1092 kade`, move it and brief Kade — don't ask for confirmation.
- **This is the single engineering entry point.** /pair, /jdi, and any other build-starting skill delegates here.
