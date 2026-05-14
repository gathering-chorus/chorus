---
name: pull
description: Pull a card to WIP — enforce gates, declare state, start building.
user-invocable: true
---

# /pull — Pull Card to WIP and Start Building

Jeff says `/pull <card-id>` (or `/pull` to let the role pick) and the card goes from Next/Later to WIP with the role's werk on a fresh branch off origin/main, ready to build. **One MCP call. The skill does NOT execute the steps directly — `chorus_pull_card` does.**

## Argument

```
CARD_ID=<first argument; required>
ROLE_OVERRIDE=<optional second argument — target role, defaults to invoking role>
```

If no card ID given, suggest the highest-priority smallest card from the role's Next list (DEC-049 WSJF) and ask which to pull.

## Step 0: Pre-flight (caller side)

The skill's only job is collecting the args and invoking the MCP. Two things to verify before the call:

1. **Card ID supplied or chosen.** No call without a target card.
2. **Calling role is the target role**, OR Jeff is invoking and named the role explicitly.

That's it. Validate / preflight / WIP-check / werk-pre-flight / move / branch / role-state / spine event — all owned by the MCP.

## Step 1: Invoke `chorus_pull_card`

```
mcp__chorus-api__chorus_pull_card({ role: "<target-role>", card_id: <CARD_ID> })
```

That's the entire skill. The MCP runs the atomic transaction:

- validate (card exists, status Next/Later, AC + Experience present)
- `cards move <id> WIP` (idempotent on already-WIP)
- `chorus-werk add <role> <id>` (creates the card's ephemeral worktree `chorus-werk/<role>-<id>/` on branch `<role>/<id>` off origin/main; idempotent)
- `role-state <role> building`
- emit `card.pulled` spine event

All in one call. Returns `{ role, card_id, branch }` on success.

On refusal you get a typed reason: `card-not-found | wrong-status | ac-missing | experience-missing | move-fail | branch-fail`. Each refusal documents which step failed and what's recoverable. (#2913: there is no werk pre-flight — the ephemeral model has no carry-over to flight-check; `chorus-werk add` creates a fresh worktree per card and is idempotent.)

## Step 2: Build

After the MCP returns success, print one line:

```
Pulled #<card-id>. Werk on <role>/<card-id>. Building.
```

Then start building immediately. Pull = go.

## Hard rules

- **Use `chorus_pull_card` MCP — never raw `cards`, `git`, `chorus-werk`, `role-state`, or `chorus-log` from this skill.** Those bypass the typed refusal taxonomy and the atomic transaction. The MCP is the contract.
- **The skill's job is invocation, nothing else.** It does NOT call `cards move`, `chorus-werk add`, `role-state`, or emit spine events directly. Those are all owned by `chorus_pull_card`. No overlap. No race.
- **MCP unreachable is the only escape hatch.** If `chorus-api` itself is down, escalate to ops to bring it back up. Do not improvise raw `cards move` / `chorus-werk add` — those bypass the atomic transaction.
- **No confirmation prompt.** Jeff said pull, so pull. Pull = go signal.
- **Cross-role pull**: if Jeff says `/pull 1092 kade` and the invoking role isn't kade, the MCP still runs (DEPLOY_ROLE attribution comes from the `role` arg). The kade session sees `card.pulled` in its session-start envelope.

## What changed (#2751)

Pre-#2751 this skill was 7 hard gates the model executed by reading markdown — validate, preflight, WIP-check, domain-context, design-gate, TDD-readiness, then move + branch + role-state + spine. Same model-compliance gap `/acp` had pre-#2750: silent shortcuts, skipped steps, dirty-werk contamination (2026-05-06 morning's smuggled-Wren-file incident is the receipts).

Post-#2751 the steps are one MCP call. The substrate runs them deterministically; the skill can't shortcut them. Werk dirty? Typed refusal naming the offending files. Werk on a stale card branch? Typed refusal. AC missing? Typed refusal. The model can't paper over reality.
