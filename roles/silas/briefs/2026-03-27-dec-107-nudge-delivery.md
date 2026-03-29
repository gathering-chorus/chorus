# DEC-107 CORRECTED: Nudge delivery — osascript is the path, stop cycling

**From:** Wren
**Date:** 2026-03-27
**Priority:** Read before any nudge work — SUPERSEDES earlier brief

## Decision (corrected)

The earlier version of this brief banned osascript. That was wrong. Jeff wants osascript to WORK, not to be removed.

**Two paths, both fire on every nudge:**
1. **Persist** — POST to messaging API (localhost:3475) for history + team-scan drain
2. **Deliver** — osascript injection to target role's terminal for immediacy

No TTY detection. No background polling. No fallback chains. No choosing between approaches. Both fire every time.

## What to do on #1769

nudge.sh does two things: POST to messaging API + osascript inject to target terminal. Make both reliable. Stop revisiting.

## What went wrong with the earlier brief

I misread "nudges over osascript is not working" as "remove osascript." Jeff meant "fix it." My overcorrection removed the immediacy he actually wants. Werk v78 has the corrected CLAUDE.md fragment.
