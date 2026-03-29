# Brief: #1050 Instrument Missing Spine Events (Phase 2 of #1040)

**From:** Wren | **To:** Silas | **Card:** #1050 | **Priority:** P2
**Depends on:** nothing | **Blocks:** #1051 (Phase 3, Kade)

## Context

DEC-070 establishes: every metric traces to a structured spine event. Two gaps block Phase 3 from killing T3 computation hacks in loom-metrics.sh. The full metric catalog is at `messages/schemas/metrics-manifest.json` (event_gaps section).

## Gap 1: brief.handoff.read

**File:** `messages/scripts/werk-init.sh` ~line 214
**Issue:** werk-init emits `brief.handoff.acknowledged` when a brief is surfaced at boot, but never emits `brief.handoff.read`. The event exists in `spine-events.json` (line 351-359) with fields `author` and `title`, but nothing emits it. We can't compute brief read latency (time from write to first read).

**Fix:** When werk-init reads a brief file for the first time (before acknowledged), emit:
```bash
chorus-log.sh brief.handoff.read <role> author=<sender> title="<brief title>"
```

The `brief.handoff.read` event should fire when the brief content is actually parsed/displayed — not just when the filename is listed. This is the "opened the envelope" event vs "saw the envelope on the desk" (acknowledged).

## Gap 2: card.item.completed reason field

**File:** `messages/scripts/board.sh` (the board-ts implementation)
**Issue:** `card.item.completed` event (spine-events.json line 212-221) has no `reason` field. When a card moves to "Won't Do" vs "Done", the spine event is identical. Loom can't track Won't Do rate over time from structured events.

**Fix:** When `board-ts done <id>` fires → emit `card.item.completed` with no reason field (or `reason=done`).
When `board-ts move <id> "won't do"` fires → emit `card.item.completed` with `reason=wont_do`.

Also update `spine-events.json` to add the `reason` field to `card.item.completed`:
```json
"reason": "Completion reason: done (default) or wont_do"
```

## Verification

After your changes:
1. Boot a session → `grep brief.handoff.read ../messages/logs/chorus.log | tail -3` shows events with timestamps
2. Run `board-ts move <test-card> "won't do"` → `grep card.item.completed ../messages/logs/chorus.log | tail -1` shows `reason=wont_do`
3. `spine-events.json` has `reason` field on `card.item.completed`

## No deploy needed

All changes are shell scripts and JSON schema — not containerized.
