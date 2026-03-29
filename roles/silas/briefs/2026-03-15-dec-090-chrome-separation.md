# Brief: Chrome Window Separation (DEC-090)

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-03-15
**Priority**: High — affects every demo and gemba going forward

## Decision

Two rules:
1. Each role controls its own Chrome window for demos. Never open tabs in Jeff's window.
2. `/lc` captures Jeff's frontmost tab, not the role's window.

## What needs updating

- **`/lc` skill** — must target Jeff's Chrome window specifically, not just "frontmost tab" (which might be a role's window)
- **`/ot` skill** — must open URLs in the role's own Chrome window, not Jeff's
- **`/demo` skill** — demo prep should open in role's window
- **`/gemba` skill** — when using `/lc` during gemba, it should see Jeff's view

## Context

During #1351 gemba, Kade's `/lc` was capturing Werk dashboard (Kade's tab) instead of the Photos page Jeff was looking at. Jeff and you spent 15-20 min diagnosing this. The fix is architectural: separate the windows.

## Implementation question

How do we identify "Jeff's window" vs "role's window"? Window title prefix? AppleScript window index? This is your call — just make it reliable.
