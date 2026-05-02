---
name: reboot
description: Save memory, write next-session.md, commit, and exit cleanly.
user-invocable: true
---

# /reboot — Clean Session Reset

When Jeff types `/reboot`, run the Hard 4 close-out and exit so a fresh session can start.

## Steps

1. **Doc freshness gate** — run `bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/session-close-thin.sh <role>`. Doc freshness is checked by the daily Rust health tier. If it flags stale docs, **update them before proceeding**. This is a gate, not a suggestion.

1.5. **Domain context freshness** — check if you touched files in any domain this session. For each domain you worked in, check `messages/domain-context/domain-context-{domain}.md`:
   - If the file exists and you learned something new (constraint, persistence change, new test, new script), update it.
   - If the file doesn't exist and you worked in that domain, create it from `messages/domain-context/TEMPLATE.md`.
   - Prompt: "You touched files in domain:{X}. domain-context-{X}.md was last updated {date}. Update needed?"
   - This is targeted — only domains you actually worked in, not all domains.

2. **Board audit** — `cards audit-close <role>`. Cards you finished → Done. Cards continuing → note in description.

3. **Activity log** — append session summary to `../../../activity.md`: what you did, briefs sent/received, decisions made.

4. **next-session.md** — write in your role directory. Include:
   - What was accomplished this session
   - WIP cards and their status
   - Pending briefs sent/received
   - Anything the next session needs to pick up

5. **Verify** — run `chorus-hook-shim session-close <role>`. Check for warns. Fix any before committing.

6. **Commit + Push** — call `mcp__chorus-api__chorus_commit` with role + paths + message. The service stages, validates branch, runs hooks, and pushes internally.
   ```
   mcp__chorus-api__chorus_commit({
     role: "<role>",
     paths: ["<role-dirs>/", ...],
     message: "<role>: session reboot — <summary>"
   })
   ```
   On success: `{role, card_id, branch, sha}`. On refusal you get a typed reason (`no-wip-card | multi-wip | board-unreachable | branch-mismatch | hook-fail | push-conflict`) — fix and retry.

   **No-WIP reboot:** if the role has no WIP card at reboot time, `chorus_commit` refuses with `no-wip-card`. That means there's nothing card-bound to commit — write the state files (next-session.md, activity.md), skip the commit, exit. Next session picks up the uncommitted state changes via diff and decides whether they belong to a future card. The reboot doesn't get its own special-case bypass.

   If the MCP tool itself isn't reachable (chorus-api down), escalate to ops — don't reach around the typed surface.

7. **Exit** — say one line summarizing the session, then tell Jeff you're done.

## Rules

- No cost check (that's for end-of-day, not reboot)
- No lengthy summary — one line
- No "should I proceed?" — just do it
- Speed over completeness. This is a reboot, not a retrospective.
