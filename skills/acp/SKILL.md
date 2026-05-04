---
name: acp
description: Accept card + commit + push — full acceptance flow in one command.
user-invocable: true
---

# /acp — Accept, Commit, Push

One command for the full acceptance flow. Jeff says `acp <card-id>` and the card is committed, pushed, and marked Done.

## Arguments

```
CARD_ID=<first argument>
```

If no card ID given, check for a card currently in demo state or the most recent demo brief.

## Step 0: Demo gate (DEC-048)

Before accepting, verify a demo happened. Check for:
1. A demo brief in your `briefs/` directory referencing this card (e.g., `*demo*${CARD_ID}*`)
2. Or Jeff explicitly saying "accept" / "acp" after seeing the work live

If **neither** exists and **you are the building role** (not Jeff or Wren accepting):
- **STOP.** Do not proceed.
- Say: `#${CARD_ID} needs a demo before acceptance. Run /demo ${CARD_ID} first.`
- Exit the skill.

If **Jeff or Wren** is running /acp, they are the accepting authority — proceed (they've seen it or are overriding).

## Step 1a: Pull + rebase via chorus_pull MCP tool

**Pull before commit so the local branch is at-or-ahead-of origin.** Mode-A means a peer's checkout could have moved HEAD between your last sync and now; pulling first reduces push-side surprises. The tool handles flock + check_branch + rebase via `git-queue.sh do_pull`.

```
mcp__chorus-api__chorus_pull({
  role: "<your-role>",
})
```

On success: `{role, card_id, status: "fetched"}`. On refusal:
- `rebase-conflict` → resolve manually, retry. do_pull aborted to pre-rebase state; spine emitted `chorus_pull.rebase.aborted`.
- `flock-timeout` → another role holds the lock; wait + retry.
- `dirty-tree` → uncommitted edits block pull-rebase. Commit or stash, then retry.
- `pull-fail` → fallback for network / auth / unmatched. Read stderr in the error message.

If pull refuses, fix the cause and retry **before** chorus_commit — committing on a stale branch produces the very push-conflict the chorus_pull step exists to prevent.

## Step 1b: Commit + push via chorus_commit MCP tool

**The card is still in WIP at this step — that's deliberate.** `chorus_commit` derives the active card from the board (status=WIP, owner=role). Marking Done first would empty the role's WIP. Commit before accept; if commit refuses, the card stays in WIP and you investigate.

Call the MCP tool with role + paths + commit message. The service handles staging, branch validation, hooks, and push internally:

```
mcp__chorus-api__chorus_commit({
  role: "<your-role>",
  paths: ["<your-dirs>/", ...],   // e.g., ["roles/kade/", "platform/api/src/"]
  message: "<your-role>: acp #${CARD_ID} — <short description>"
})
```

On success the response is `{role, card_id, branch, sha}`. On refusal you get a typed reason: `hook-fail | commit-fail | push-conflict | push-fail`. Each refusal is a clear next-step:
- `hook-fail` → fix what pre-commit reported, retry
- `commit-fail` → non-hook commit failure (read stderr); fix and retry
- `push-conflict` → rebase has a real conflict; chorus_pull first to resolve, then retry
- `push-fail` → fallback for non-conflict push failure (read stderr)

If the MCP tool itself isn't reachable (chorus-api down or pre-deploy), the acceptance can't proceed — escalate to ops to bring chorus-api back up before retrying. Don't reach around the typed surface.

## Step 2: Mark the card Done

Now that the work is on main:

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards done ${CARD_ID}
```

## Step 3: Emit spine event

```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log card.accepted <your-role> card=${CARD_ID}
```

## Step 3.5: State file sync (if Wren)

If Wren is running /acp, check if `projects.md` or `backlog.md` need updating based on what just shipped. Update inline — don't defer to close-out.

## Step 4: Confirm

One line:

```
Accepted #<card-id> — committed and pushed (sha=<sha>).
```

## Rules

- No confirmation prompt — Jeff said accept, so accept
- If the card isn't in WIP or Demo state, warn but still proceed (Jeff overrides)
- **Commit before accept.** Card stays WIP through the commit so chorus_commit can derive it from board state.
- Always emit the spine event after marking Done
- **Use `mcp__chorus-api__chorus_commit` — never raw git from skills.** The MCP tool wraps the canonical substrate, returns typed refusals, and binds the commit to the board's WIP card. Reaching around the typed surface bypasses the refusal taxonomy and the board-derived card binding.
- **Branch ops go through the typed adapter (#2706 #2710 #2712).** If you need to switch branches (e.g., back to main after PR merge, onto a card branch), use `bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/git-queue.sh checkout <branch>` — never raw `git checkout`. The adapter serializes under the same flock as commit/push/pull, closing the shared-HEAD race. Once #2711 ships, raw `git checkout` is hook-refused.
