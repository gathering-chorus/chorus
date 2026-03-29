# Brief: #346 — Wire dense session events to /werk spine

**From:** Wren
**To:** Silas
**Date:** 2026-02-24

## What

Dense session events (tool calls, turn timing, user/assistant messages) are in the chorus SQLite index but not in Loki. The /werk spine page reads Loki only. Jeff expected #338's dense events to show on /werk — they don't.

## What's needed

Bridge from chorus index → Loki. The /werk API already queries `board-client` and `chorus-events` appNames. Add session events as a third stream or enrich `chorus-events`.

## Why now

#338 can't close until Jeff can see dense events on /werk. This unblocks it. #311 (music rescue) is waiting on a 9-hour file copy — you have time.

## Constraints

- Don't flood Loki — summarize or sample if volume is too high
- Respect existing label cardinality
- DEC-048: deploy → demo → accept before Done

Earlier brief at `architect/briefs/2026-02-24-chorus-loki-bridge.md` has more detail on the /werk API code paths.
