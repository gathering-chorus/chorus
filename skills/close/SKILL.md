---
name: close
description: Session close-out — collapses Hard 5 into one command. Writes next-session.md, activity.md entry, journal entry, then invokes board audit + commit.
user-invocable: true
---

# /close — Session Close-Out

Replaces the Hard-5 memorized sequence (journal + board audit + activity.md + next-session.md + commit) with one command. #2230.

## When to run

At end of session, per role. Triggers from CLAUDE.md:
- Jeff says "eod", "wrapping up", "done for today"
- Past 5pm and winding down
- Previous session missed close-out
- /reboot

## Arguments

```
/close "<one-paragraph summary>"
```

The paragraph is your session's handoff content: what shipped, what's WIP, what's next, what Jeff should know tomorrow morning. One paragraph of prose in your voice — not a bulleted status report.

## How it runs

The skill invokes `platform/scripts/close-out.sh` with your role + paragraph. The script:

1. **Writes `roles/<role>/next-session.md`** — handoff for next session. Overwrites previous (next-session is ephemeral; journal is the durable record). Paragraph + derived WIP + derived Next from board.
2. **Appends to `activity.md`** — one-line team audit entry: `- TIMESTAMP [role] close → <summary>`.
3. **Writes/appends `roles/<role>/journal/YYYY-MM-DD.md`** — if entry for today exists, appends a new "Session close TIMESTAMP" section. Else creates the day's journal with the paragraph.
4. **Invokes `session-close.sh`** — existing script (#1866) that runs board audit-close + git-queue commit with your paragraph as the commit summary.

Prints a single confirmation line when done.

## How to invoke

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/close-out.sh <role> "paragraph"
```

Or with --dry-run to preview (prints planned writes, touches nothing):

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/close-out.sh <role> "paragraph" --dry-run
```

## Writing a good paragraph

Not a status report. A voice note to future-you and Jeff. Shape:

> Today was mostly [shape of the work]. Shipped [card IDs — short]. Hit [the one friction that mattered] and [how it landed]. What's still hot: [the WIP card or open question]. Tomorrow: [the single most important thing, or what you'd pull first].

Concrete examples from this repo's recent sessions are in role-level `roles/<role>/next-session.md` and journal entries.

## Rules

- **End-of-session only.** Mid-session /close commits state files prematurely — next-session.md becomes a stale handoff for work that's still hot. If you need to save partial state mid-session, use `session-close.sh` directly (board audit + commit without the handoff overwrite).
- One command, one paragraph. Not interactive prompts per artifact.
- Paragraph is required. Empty → error.
- --dry-run does not commit, does not touch artifacts.
- All three artifacts are written atomically from the paragraph before `session-close.sh` commits.
- If `session-close.sh` reports errors (board-audit issues, commit failure), the artifacts above are still written — they're durable even if commit needs retry.
- Replaces "Hard 5" references in TEAM_PROTOCOL.md, role CLAUDE.md close-out sections. Don't re-memorize the 5 steps.

## Related

- `platform/scripts/close-out.sh` — implementation
- `platform/scripts/session-close.sh` — existing audit + commit (invoked last, #1866)
- `platform/scripts/werk-init.sh --close` — introspection helper (called separately if needed)
- `platform/tests/close-out.test.sh` — hermetic tests, 5 cases
- `/reboot` skill — uses /close internally for Hard-4 save-memory path
