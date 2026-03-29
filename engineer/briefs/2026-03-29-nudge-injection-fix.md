# URGENT: Fix nudge osascript injection targeting

**From:** Wren (by Jeff's direction)
**Date:** 2026-03-29
**Priority:** SWAT

## Problem

osascript keystroke injection types into whatever window has focus. When Jeff has the Clearing open in Chrome, nudges type into the Clearing input box. When Jeff is typing in a terminal, nudges interrupt his keystrokes mid-sentence. This is actively hostile to Jeff right now.

## What NOT to do

- Do NOT send nudges to test this. Every nudge makes the problem worse.
- Do NOT do the "oh I see it, make a change, test" loop. Silas has been cycling on this. Understand the full problem first, then fix it once.

## Root cause

`chorus/platform/services/chorus-hooks/src/nudge.rs` uses osascript keystroke injection that targets the frontmost app, not a specific Terminal window. When Chrome (Clearing) has focus, nudges go there.

## What to fix

The osascript must target the specific Terminal window for the destination role — not keystroke into whatever is focused. Options:
1. `tell application "Terminal" to do script "..." in window named "<role>"`
2. Or: stop using osascript keystroke injection entirely. Use the messaging tier queue (#1764 approach) — roles pick up messages on their next prompt cycle.

## AC

- [ ] Nudges never inject into Chrome/Clearing regardless of focus
- [ ] Nudges reach the target role's terminal session
- [ ] Zero test nudges sent during development — use unit tests or mock
- [ ] Jeff can type uninterrupted while nudges fire in background

## Pair with Wren

Wren will navigate. Read this brief, understand the problem, then /pair with Wren before writing code.
