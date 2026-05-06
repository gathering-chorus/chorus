---
name: unpull
description: Reverse a pull — return the card to Next, tear down the branch, idle the role.
user-invocable: true
---

# /unpull — Reverse a Pull

Jeff says `/unpull <card-id>` (or `/unpull` to unpull the role's current WIP) and the card goes from WIP back to Next, the werk detaches to origin/main, the local + remote branches are deleted, role-state goes idle, and a `card.unpulled` spine event is emitted. **One MCP call. The skill does NOT execute the steps directly — `chorus_unpull_card` does.**

## Argument

```
CARD_ID=<first argument; required>
```

If no card ID given, derive from the role's current WIP card.

## Step 0: Pre-flight (caller side)

The skill's only job is collecting the args and invoking the MCP. One thing to verify before the call:

1. **Card ID supplied or derivable from current WIP.** No call without a target.

That's it. Validate / werk-pre-flight / move / branch teardown / role-state / spine — all owned by the MCP.

## Step 1: Invoke `chorus_unpull_card`

```
mcp__chorus-api__chorus_unpull_card({ role: "<your-role>", card_id: <CARD_ID> })
```

That's the entire skill. The MCP runs the atomic transaction:

- validate (card is WIP and owned by role)
- werk pre-flight (refuses `werk-dirty` so you don't lose uncommitted work)
- `cards move <id> Next`
- `chorus-werk close <role> <id>` (detach + branch teardown + `werk.detached` spine event)
- `role-state <role> idle`
- emit `card.unpulled` spine event

All in one call. Returns `{ role, card_id, prior_branch, branch_closed }`.

On refusal you get a typed reason: `card-not-found | wrong-status | wrong-owner | werk-dirty | move-fail | branch-close-fail`. Each refusal documents which step failed and what's recoverable.

## Step 2: Acknowledge

After the MCP returns success, print one line:

```
Unpulled #<card-id>. Werk detached, branch <prior_branch> torn down. Idle.
```

Stop. The pull is reversed; nothing else to do.

## Hard rules

- **Use `chorus_unpull_card` MCP — never raw `cards move`, `git`, `chorus-werk`, `role-state`, or `chorus-log` from this skill.** Those bypass the typed refusal taxonomy and leave stale state — that's the exact pattern this card was filed to fix.
- **No confirmation prompt.** Jeff said unpull, so unpull.
- **Werk-dirty refusal is the safety net.** If you have uncommitted work, the MCP refuses and tells you which files. Commit, stash, or explicitly abandon them yourself — don't pass a flag to suppress the refusal (there isn't one).

## What this fixes (#2759)

`/pull` had no inverse. The team's pattern was `/pull <id>` → change of mind → `cards move <id> Next` → walk away. That left:
- werk on the dead branch
- local + remote branch refs persisting
- `role-state` still saying `building`
- no spine signal that the card was abandoned

Today's session produced 5 stale branches from this pattern in one afternoon. `/unpull` makes the teardown atomic and observable so abandoned-mid-flight is a first-class state, not orphaned debt.
