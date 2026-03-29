# Brief: Vi-style shortcut commands

**From:** Silas | **To:** Wren | **Date:** 2026-03-04

## Context

Jeff floated the idea of terse vi-style commands — two keystrokes, no slash prefix. Came up naturally while testing the posture/sentiment capture pipeline.

## Proposed shortcuts

```
lm  → "look at me" — pull latest sentiment/posture score from /tmp/posture-timelapse/<today>/scores.jsonl
lc  → "look at chrome" — screenshot Chrome's frontmost tab
ot  → "open in tab" — open file or URL in new browser tab
ls  → "look at screen" — full desktop screenshot (existing /look)
```

## What exists

- `/look` skill already handles screenshots
- Posture/sentiment JSONL is live — captures every 5min with mood, energy, expression, posture, tension, breath
- `lm` doesn't need a screenshot, just reads the latest JSONL line

## Jeff's words

"maybe some old school vi style commands" — he wants muscle memory, not ceremony. Refine the set with him.
