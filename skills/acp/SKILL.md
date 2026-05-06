---
name: acp
description: Accept card + commit + push — full acceptance flow in one command.
user-invocable: true
---

# /acp — Accept, Commit, Push

Jeff says `acp <card-id>` (or `acp` to accept the current WIP card) and the card goes from werk to merged-on-main with the card marked Done. **One MCP call. The skill does NOT execute the steps directly — `chorus_acp` does.**

## Argument

```
CARD_ID=<optional first argument>
```

If omitted, `chorus_acp` derives the card from the board (`status=WIP`, `owner=role`).

## Step 0: Demo gate (DEC-048)

Before invoking `chorus_acp`, verify a demo happened:

1. Card has `demo:preflight-pass` comment AND gate-pass comments, OR
2. Jeff or Wren explicitly said "accept" / "acp" after seeing the work

If neither and **you are the building role** — **STOP**. Run `/demo ${CARD_ID}` first.

If Jeff or Wren is invoking, they're the accepting authority — proceed.

## Step 1: Invoke `chorus_acp`

```
mcp__chorus-api__chorus_acp({ role: "<your-role>" })
```

That's the entire skill. The MCP runs the atomic transaction:

- commit + push (via the existing `chorus_commit` substrate)
- `gh pr view` (detect existing PR) → `gh pr create` (if missing)
- `gh pr merge --squash --delete-branch`
- `cards done <card-id>`
- emit `card.accepted` spine event
- `chorus-werk close <role> <card-id>` (when `CHORUS_WERK_ENABLE=1`)

All in one call. Idempotent on re-run (PR-already-exists / branch-already-closed are detected and skipped).

On success: `{ role, card_id, sha, pr_url, branch_closed }`. Print one line:

```
Accepted #<card_id> — committed, merged, branch closed (sha=<sha>).
```

On refusal you get a typed reason: `hook-fail | commit-fail | push-conflict | pr-create-fail | pr-merge-fail | cards-done-fail`. Each refusal documents which step failed and what's recoverable.

## Hard rules

- **Use `chorus_acp` MCP — never raw git, gh, cards CLI, or chorus-log from this skill.** Those bypass the typed refusal taxonomy and the atomic transaction. The MCP is the contract.
- **The skill's job is pre-flight + invocation.** It does NOT call `chorus_pull`, `chorus_commit`, `gh pr merge`, `cards done`, `chorus-werk close`, or emit spine events directly. Those are all owned by `chorus_acp`. No overlap. No race.
- **MCP unreachable is the only escape hatch.** If `chorus-api` itself is down, escalate to ops to bring it back up. Do not improvise raw git — that's how today's silent stale-branch class ate 18 refs before Jeff noticed.
- **No confirmation prompt.** Jeff said acp, so acp.
- **No self-acp on code cards (DEC-048).** The demo gate at Step 0 enforces this — builder must have a demo brief or explicit Jeff/Wren acp word before the MCP fires.

## What changed (#2750)

Pre-#2750 this skill was 7 steps the model executed by reading markdown — pull, commit, push, mark done, etc. The model demonstrably shortcut, skipped, or improvised steps. Today the steps are one MCP call. The substrate runs them deterministically; the skill can't shortcut them.

Migration day post-#2750: any role with `CHORUS_WERK_ENABLE=1` runs `/acp` and the entire transaction lands without any manual gh-merge or branch-cleanup. The substrate closes the loop.
