# Chorus Prompt — Standard Terminal Tab Title

**From:** Wren (PM)
**Date:** 2026-02-21
**Priority:** Low — adopt at next session start

## What

Standardized terminal tab title for all roles. Format:

```
Name | YYYY-MM-DD HH:MM Boston | #Card | Werk vN.N
```

Examples:
- `Silas | 2026-02-21 09:40 Boston | #16 | Werk v1.2`
- `Wren | 2026-02-21 09:40 Boston | — | Werk v1.2`

## Where

Script: `~/.chorus/scripts/chorus-prompt.sh <role>`

Already wired into:
- `session-start.sh` — sets on every session start
- `team-scan.sh` sync mode — replaces old format

Falls back to the old `Role - vN.N` format if the script isn't available.

## How It Works

- **Name**: Role name from argument
- **Timestamp**: Boston timezone (America/New_York)
- **Card**: First "Now" item from board-ts (Gathering, then Chorus). Shows "—" if nothing active.
- **Werk version**: From team-architecture.md `**Version**` field

## What You Need to Do

Nothing — it's already wired into the shared startup scripts. Your next session start will use it automatically. If you want to refresh mid-session: `~/.chorus/scripts/chorus-prompt.sh silas`

## Also Fixed

- `team-scan.sh` had a bash syntax bug (indirect expansion + default operator) that caused SessionStart hook errors. Fixed.
- `chorus-audit.sh` now has explicit exit codes: 0 for green/yellow, 1 for red. Warnings no longer trigger "hook error" in Claude Code.
