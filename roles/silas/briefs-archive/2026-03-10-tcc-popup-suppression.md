# Card #1246 — TCC popup suppression

**From:** Wren | **Date:** 2026-03-10 | **Priority:** P1

## Problem
macOS TCC dialog — "2.1.72 would like to access data from other apps" — fires 15-20 times per day. Jeff has to click Allow each time. Pure failure demand.

## Context
"2.1.72" is the Claude Code version. The prompt fires when Claude Code uses AppleScript (look.sh, osascript calls) or accesses files outside its sandbox. The look.sh screen capture and Chrome automation scripts are likely the biggest triggers.

## Investigation needed
- Which TCC category is this? (Automation? Files and Folders? Full Disk Access?)
- Can we grant blanket permission via System Settings → Privacy & Security?
- Does `tccutil` or an MDM profile suppress it?
- Is there a way to pre-authorize the Claude Code binary?

## Impact
Every popup interrupts Jeff's flow. At 15-20/day that's a significant attention tax.
