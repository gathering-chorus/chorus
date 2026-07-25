---
name: wip
description: One MCP call answering "what is this role building right now" — the role's WIP card(s). user-invocable
---

# /wip — What's In Flight (#3683)

Jeff (or a role) types `/wip` and gets the role's current WIP card(s). **One MCP call. No board-scraping, no filesystem reads, no `cards` CLI.**

## Argument

```
ROLE=<optional — defaults to the invoking role>
```

## Step 1: Invoke `chorus_wip`

```
mcp__chorus-api__chorus_wip({ role: "<role>" })   // omit role to use the calling role
```

## Step 2: Paste the answer

**Focus mode rule: the returned text IS the answer — paste it verbatim into the reply.** Jeff sees only the reply; a described summary is not the tool output.

## Hard rules

- **One call, paste, done.** No fallback to `cards list` / pulse files / board scraping — the MCP is the contract.
- If the MCP errors, surface the exact error and stop. chorus-api down = ops escalation, not improvisation.
