# Demo: #346 — Chorus→Loki Bridge (Session Events on /werk Spine)

**From:** Silas
**To:** Wren
**Date:** 2026-02-24
**Card:** #346 | DEC-048 proving gate — demo step

## What shipped

Dense session events now flow from JSONL session files → Loki → /werk Spine tab. Before this, the Spine only showed lifecycle events (card moves, commits, deploys). Now it shows the **building work itself**: each turn between Jeff and a role, significant tool calls (Bash, Write, Edit, Task), and context compaction events.

## How it works

```
JSONL session files (~/.claude/projects/)
    ↓ chorus-watch-sessions.sh (fswatch, ~3s cadence)
chorus-bridge-sessions.sh (new — extracts turn summaries)
    ↓ appends to chorus.log with appName="chorus-session"
Promtail scrapes chorus.log
    ↓
Loki stores structured JSON
    ↓
/api/werk/activity queries 3 appNames in parallel
    ↓
/werk Spine tab renders interleaved: lifecycle + session events
```

## What Jeff sees

- **135 session events** on the Spine tab (from today's Wren + Kade sessions)
- **`wren turn`** entries with tool counts and duration
- **`kade used Bash`** entries showing actual commands
- New "Session activity" filter in the event dropdown
- Session events classified as Building in the scorecard
- Role colors working (green=wren, blue=kade, amber=silas)

## Files changed

1. **`~/.chorus/scripts/chorus-bridge-sessions.sh`** (new) — Python bridge script, byte-offset watermarks, turn-level summarization
2. **`~/.chorus/scripts/chorus-watch-sessions.sh`** (modified) — calls bridge after index
3. **`src/app.ts`** (modified) — third parallel Loki query for `chorus-session`, `summary` field fallback in parseSpineEntries
4. **`views/werk.ejs`** (modified) — `session_turn`, `session_tool`, `session_compact` sentence formatting + "Session activity" filter + scorecard classification

## Key design decisions

- **New appName `chorus-session`** — clean separation from lifecycle events (`chorus-events`), no Promtail config changes needed
- **No `message` field in bridge JSON** — avoids Promtail's output stage extracting it and losing the full JSON (learned the hard way — first batch was plain text)
- **Turn-level summaries, not per-tool** — keeps Loki volume sane. Significant tools (Bash, Write, Edit, Task) get individual events.
- **Watermark-based idempotency** — bridge tracks byte offsets per file, re-runs emit nothing

## Unblocks

- **#338** (Dense spine events) — Wren's card. The data Wren instrumented is now visible on /werk. #338 can close.

## Proving gate status

1. **Deploy** — running at localhost:3000
2. **Demo** — Jeff saw it live on Spine tab, walked through value with me
3. **Accept** — pending Wren's review
