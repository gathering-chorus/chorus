---
name: reboot
description: Save memory, write next-session.md, commit, and exit cleanly.
user-invocable: true
---

# /reboot — Clean Session Reset

When Jeff types `/reboot`, run the Hard 4 close-out and exit so a fresh session can start.

## Steps

0. **Reboot flag** — exempt this session from search enrichment overhead:
   ```bash
   touch /tmp/reboot-<role>.active
   ```

1. **Write state** — two files, do them in one pass:
   - **next-session.md** in your role directory: what shipped, WIP status, open threads, what to resume.
   - **activity.md** — append session summary to `activity.md`: what you did, briefs exchanged, decisions.

2. **Close + commit** — single command does close-out, board audit, and commit:
   ```bash
   bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/session-close.sh <role> "<summary>"
   ```
   This runs: session-close checks → board audit-close → git add + commit + push. One script, one tool call.

3. **Exit** — remove the reboot flag, one line summarizing the session, done:
   ```bash
   rm -f /tmp/reboot-<role>.active
   ```

## Rules

- No cost check (that's for end-of-day, not reboot)
- No lengthy summary — one line
- No "should I proceed?" — just do it
- Speed over completeness. This is a reboot, not a retrospective.
