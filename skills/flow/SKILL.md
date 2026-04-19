---
name: flow
description: Board sweep and work proposal — Wren reads the board, tails active roles, and proposes what to pull next.
user-invocable: true
---

# /flow — Board Sweep and Work Proposal

Vi-style command: one keystroke, full action. When Jeff types `/flow`, Wren sweeps the board and proposes what to pull next.

## What It Does

1. **Read the board** — WIP, Now, Next across both products
2. **Assess current state** — what's done, stalled, blocked, or ready to close
3. **Tail active roles** — `chorus-query.sh tail <role>` on anyone in WIP to see live status
4. **Propose next pulls** — for each role, recommend 1-2 cards to move to Now based on:
   - What just finished (continuation energy)
   - Which chunk is hot (spiral momentum)
   - Priority and dependencies
   - Role availability (who's idle, who's deep in work)
5. **Surface risks** — anything stale, any card without AC, any proving gap

## How to Execute

```bash
# Step 1: Board state
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards list --status wip
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards list --status now

# Step 2: Tail active roles (only those with WIP cards)
bash ~/Users/jeffbridwell/.chorus/scripts/chorus-query.sh tail kade --lines 5
bash ~/Users/jeffbridwell/.chorus/scripts/chorus-query.sh tail silas --lines 5

# Step 3: Check recent completions
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards list --status done | head -10

# Step 4: Open /flow in Chrome so Jeff can see it visually
open -a "Google Chrome" "http://localhost:3000/flow"
```

## Output Format

Keep it tight — this is Jeff's standup, not a report.

```
## Flow — [date]

**WIP:** [count] cards
[table: #, card, owner, live status from tail]

**Ready to close:** [any WIP cards that look done]

**Next options for Now:** [2-3 cards from Next that Wren recommends pulling, based on:]
- What just finished (continuation energy)
- Which chunk is hot (spiral momentum)
- Priority, dependencies, role availability
- Present as choices Jeff can pick from, not a plan to approve

**Risks:** [anything stale, blocked, or missing AC]
```

## When to Use

- Jeff types `/flow`
- Start of session after context loads
- After completing a card — what's next?
- Jeff asks "what should we work on" or "where are we"
