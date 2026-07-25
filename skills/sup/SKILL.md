---
name: sup
description: Show the role's priorities in the #3654 board-domain order — chunks by roleSequence, cards by rank, from the graph. user-invocable
---

# /sup — Show Your Priorities (#3683)

Jeff (or a role) types `/sup` and gets the role's priorities **walk** — the order the graph holds (#3654 board domain), not a label-scrape and not a recap. **One MCP call.**

The order is data: chunks in `roleSequence` order (loom position shown where declared), cards in `rank` order within each chunk — the same order every session, every role, until a governed write re-sequences it. Open cards in no chunk are listed after as **unsequenced** with their source scope — visible, never hidden.

## Argument

```
ROLE=<optional — defaults to the invoking role; Jeff can name any role>
```

## Step 1: Invoke `chorus_sup`

```
mcp__chorus-api__chorus_sup({ role: "<role>" })   // omit role to use the calling role
```

## Step 2: Paste the walk

**Focus mode rule: the returned text IS the answer — paste it verbatim into the reply.** Jeff sees only the reply; a summary or reordering defeats the whole point (the graph order IS the contract).

## Hard rules

- **Do not re-sort, filter, or editorialize the walk.** The graph decided the order (#3654 + the #3681 uniqueness floor); the skill's job is delivery.
- **No fallback** to `chorus_priorities_readout` (tagged-only label-scrape — superseded here), `cards chunk` (static-map, blind to real labels — the #3432 defect), or any board scraping.
- If the MCP errors, surface the exact error and stop.

## Re-sequencing (the other half of the loop)

/sup only READS. When Jeff says "X first" in conversation, the walk changes via a governed DAL write to the chunk's `roleSequence`/`rank` (duplicate ordinals are refused at the door) — then /sup reflects it everywhere, next call.
