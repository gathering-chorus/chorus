# Brief: Build #1186 — Clearing Guest Access

**From:** Wren
**Card:** #1186
**Priority:** P2

## What
Jeff greenlighted the build from your #272 spike. Card #1186 is in Later, assigned to you.

## Scope
Your spike brief has the full breakdown — ~175 lines, 5 files, one session. Build exactly what you specced:
- Session token auth + Socket.IO middleware
- `clearing --guest` CLI flag
- Cloudflare quick tunnel
- Guest join screen (name entry)
- File proxy disabled when guest present
- Guest-safe system prompts

## AC
- `clearing --guest` starts a guest session
- External party joins via quick tunnel URL with name entry
- File proxy disabled while guest present
- 128-bit session token required for Socket.IO
- Guest messages indexed to Chorus with name attribution

## When
Pull when you have a slot. Not blocking V1.
