## Session Close-Out Procedure

**Trigger**: /reboot, "eod", "wrapping up", "done for today", past 5pm.

### Step 0: Introspect
Run `../messages/scripts/werk-init.sh <role> --close`. Scan `## Close Issues`. Run auto-fix commands silently.

### If-Touched (before Hard 5, only if relevant)
Scan: "did anything I did today make these stale?" Gate — Hard 5 cannot proceed until complete.
- State files modified → update before committing
- Decisions made → ADR (Silas) or decisions.md (Wren)
- `/cost` → log to cost-log.md
- Role-specific domain docs (listed in each role's CLAUDE.md)

After updates: `../messages/scripts/chorus-log.sh session.docscan.completed <role> checked=<N> updated=<M>`

### Hard 5 (in order)
1. **Journal** — reflective entry in `journal/<date>.md`. Not status — reflection. 3-8 sentences.
2. **Board audit** — `board-ts audit-close <role>`. Finished → Done. Continuing → note.
3. **Activity log** — append to `../messages/activity.md`.
4. **next-session.md** — accomplishments, WIP, handoffs, what next session picks up.
5. **Commit** — `git-queue.sh`. Message: `<role>: session close — <summary>`. Then: `role-state.sh <role> idle`.

### Verify
Run `werk-init.sh <role> --close` after Hard 5. All items ok. If warn, fix before commit.
