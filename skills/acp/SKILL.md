---
name: acp
description: Accept card + commit + push — full acceptance flow in one command.
user-invocable: true
---

# /acp — Accept, Commit, Push

Invoke `chorus_acp`. If the user typed `/acp <card-id>`, pass it as the intent assertion. Print the result.

```
# /acp 2847 — pass intent
mcp__chorus-api__chorus_acp({ role: "<your-role>", card_id: 2847 })

# /acp — derive from branch
mcp__chorus-api__chorus_acp({ role: "<your-role>" })
```

That's the entire skill. The MCP enforces card-derivation, werk-state, demo evidence (if required), commit + push, PR merge, cards-done, branch closure, and spine emission. When `card_id` is passed, MCP refuses with `card-mismatch` if the branch-derived id differs (#2868). Refusal taxonomy and return shape are documented on the MCP tool itself — read it there, not here.

If `chorus-api` is unreachable, escalate to ops. Do not improvise raw git/gh/cards CLI from this skill — PreToolUse hooks refuse those subprocess paths from agent sessions.
