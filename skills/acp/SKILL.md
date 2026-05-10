---
name: acp
description: Accept card + commit + push — full acceptance flow in one command.
user-invocable: true
---

# /acp — Accept, Commit, Push

Invoke `chorus_acp`. Print the result.

```
mcp__chorus-api__chorus_acp({ role: "<your-role>" })
```

That's the entire skill. The MCP enforces card-derivation, werk-state, demo evidence (if required), commit + push, PR merge, cards-done, branch closure, and spine emission. Refusal taxonomy and return shape are documented on the MCP tool itself — read it there, not here.

If `chorus-api` is unreachable, escalate to ops. Do not improvise raw git/gh/cards CLI from this skill — PreToolUse hooks refuse those subprocess paths from agent sessions.
