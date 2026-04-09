---
name: golfball
description: Pre-sequence scan for fix/swat cards in a domain before building new features. "Are there golfballs in the fairway?"
user-invocable: true
---

# /golfball — Domain Safety Check

Before building something new in a domain, check for unresolved fix/swat cards (golfballs) that might block flow or invalidate the new work.

**Any role can invoke this.** Jeff uses it before planning. Builders use it before pulling.

## Arguments

```
/golfball <domain>    — scan one domain
/golfball             — scan all domains with active new/enhance work
```

Domain names match board labels: `photos`, `music`, `chorus`, `documents`, `people`, `property`, `search`, `infrastructure`, `convergence`, `seeds`, etc.

## How to Execute

### Step 1: Get board state

```bash
CARDS="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards"
BOARD_OUTPUT=$(bash "$CARDS" list 2>/dev/null)
```

### Step 2: Determine scan scope

**If domain argument given:** Scan that domain only.

**If no argument:** Find all domains with `type:new` or `type:enhance` cards in WIP or Next. These are the "fairways" — domains where new work is happening.

```bash
# Extract WIP + Next cards
ACTIVE=$(echo "$BOARD_OUTPUT" | sed -n '/^WIP /,/^[A-Z]/p; /^Next /,/^[A-Z]/p' | grep -E '^\s+\d+')

# Find domains with proactive (new/enhance) work
FAIRWAYS=$(echo "$ACTIVE" | grep -E 'type:new|type:enhance' | grep -oE 'domain:\w+' | sed 's/domain://' | sort -u)
```

### Step 3: Scan for golfballs

For each domain in scope, find `type:fix` and `type:swat` cards in **WIP, Now, and Next** (not Later — that's inventory, not impediments):

```bash
for DOMAIN in $SCAN_DOMAINS; do
  # Extract WIP + Now + Next sections only
  ACTIVE_CARDS=$(echo "$BOARD_OUTPUT" | sed -n '/^WIP /,/^[A-Z]/p; /^Now /,/^[A-Z]/p; /^Next /,/^[A-Z]/p')
  # Count golfballs — fix/swat cards in this domain
  GOLFBALLS=$(echo "$ACTIVE_CARDS" | grep "domain:$DOMAIN" | grep -E 'type:fix|type:swat')
  COUNT=$(echo "$GOLFBALLS" | grep -cE '^\s+\d+' 2>/dev/null || echo 0)
done
```

**Why not Later?** Auto-detected DEFECT cards accumulate in Later (195+ as of April 2026). Including them inflates the count with transient log errors nobody will act on. Later-level awareness belongs in the daily scan, not the pre-build safety check.

### Step 4: Report

**Format — one section per domain scanned:**

```
## <domain>

**Proactive work:** #1234 (new), #1235 (enhance)
**Golfballs:** N

  #1100 [fix] Title — owner [status]
  #1102 [swat] Title — owner [status]

Recommendation: N golfballs in <domain> — consider addressing before new feature work.
```

**If no golfballs in a domain:**
```
## <domain>

Fairway clear — no fix/swat cards.
```

**If no argument and no active new/enhance work anywhere:**
```
No active new/enhance cards on the board — nothing to scan.
```

### Step 5: Emit spine event

```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log golfball.scan.completed <your-role> domain=${DOMAIN} golfballs=${COUNT}
```

## When to Use

- **Before pulling a new feature card** — is this domain clear?
- **Before starting a new sequence** — what's lurking?
- **During triage** — should we clean up before building?
- **In the daily scan** — the automated version of this runs in #2088

## Rules

- Report facts, not judgment. List the golfballs and let Jeff decide.
- Include card status — a fix card in Later is less urgent than one in Now.
- Don't filter out auto-detected defect cards — they count. If 20 DEFECT cards clutter a domain, that IS the finding.
- Scan WIP, Now, and Next only. Later is inventory — the daily scan covers it. A golfball is an impediment in the fairway, not a ball in the rough.
- If the domain argument doesn't match any board labels, say so: "Unknown domain: <X>. Valid domains: <list>."
