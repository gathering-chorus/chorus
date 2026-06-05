---
name: pair-heartbeat-check
description: Pair navigator attention monitor tick — internal skill invoked by cron, not by Jeff directly. Detects navigator silence via pulse-gather and escalates 60s→120s→180s.
---

# /pair-heartbeat-check — Navigator Attention Monitor Tick

Internal skill — the driver's cron invokes it every minute during a `/pair` session
(#1897 enforcement). It is NOT invoked by Jeff directly.

## What it does (#2317)

Detects whether the navigator has gone silent and escalates. The activity signal is
**`pulse-gather <navigator>`** (#3205) — the navigator's real tool-turn stream, not a
self-reported timestamp. This is the fix for the old bespoke monitor: it tracked
`/tmp/pair-nav-last-activity` + scratch-file mtime, so a navigator actively editing
files (clearly working) would still be flagged "silent" if they hadn't touched the
scratch file. The stream never lies — if the navigator ran a tool this minute,
pulse-gather shows it.

Because the cron fires every 60s and pulse-gather emits only turns since the last
tick, an **empty** poll == the navigator produced no tool turn this minute. Consecutive
empty ticks count the silence; any non-empty poll resets it.

## Arguments

```
/pair-heartbeat-check <card-id> <navigator-role>
```

## How to Execute

Run this block. It is the whole tick — deterministic, no improvisation:

```bash
CARD_ID="$1"; NAV="$2"
SILENT_FILE="/tmp/pair-heartbeat-${CARD_ID}-silent"
SCRATCH="/tmp/pair-${CARD_ID}.md"

# 1. Poll the navigator's live activity (per-observer cursor — won't steal a gemba's deltas).
GATHER="$(DEPLOY_ROLE=${ROLE:-wren} pulse-gather "$NAV" 2>/dev/null)"

# 2. Scratch-file write by the navigator also counts as activity (directions = engaged).
SCRATCH_FRESH=0
if [ -f "$SCRATCH" ]; then
  SCRATCH_MOD=$(stat -f %m "$SCRATCH" 2>/dev/null || echo 0)
  LAST_SEEN=$(cat "/tmp/pair-heartbeat-${CARD_ID}-scratch" 2>/dev/null || echo 0)
  [ "$SCRATCH_MOD" -gt "$LAST_SEEN" ] && SCRATCH_FRESH=1 && echo "$SCRATCH_MOD" > "/tmp/pair-heartbeat-${CARD_ID}-scratch"
fi

# 3. Active this tick? (fresh turns OR scratch write). "rebuilding" (stream absent) is
#    NOT a stall — never escalate on missing data; treat as unknown → reset.
if [ -n "$GATHER" ] || [ "$SCRATCH_FRESH" = "1" ]; then
  echo 0 > "$SILENT_FILE"        # active → reset the silence clock
  exit 0                          # silent tick (the good kind)
fi

# 4. Quiet this tick → count consecutive silence (each tick ≈ 60s, cron is */1).
SILENT=$(( $(cat "$SILENT_FILE" 2>/dev/null || echo 0) + 1 ))
echo "$SILENT" > "$SILENT_FILE"
```

Then escalate by the silence count (advisory, not blocking):

- **1 tick (~60s):** print `⚠ Navigator ${NAV} silent ~60s on #${CARD_ID} — thinking, blocked, or disengaged?`
- **2 ticks (~120s):** re-nudge the navigator — `nudge ${NAV}: "[pair] silent ~2min on #${CARD_ID} — blocked or lost context?"`
- **3+ ticks (~180s):** escalate —
  ```bash
  /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pair.navigator.stall ${ROLE:-wren} card=${CARD_ID} nav=${NAV} elapsed=$((SILENT*60))s
  ```
  append `## Escalation` to the scratch file, and nudge Wren for gemba.

## Cleanup (driver does this at pair end)

```bash
# CronDelete the heartbeat check
rm -f /tmp/pair-heartbeat-<card-id>-silent /tmp/pair-heartbeat-<card-id>-scratch
```

## Rules

- The activity signal is pulse-gather — one path, the same verb gemba and /pair MONITOR use. Do not re-derive navigator activity by hand-reading observations.jsonl.
- "rebuilding" (stream absent, e.g. post-reboot) is never treated as a stall.
- Escalation is advisory — it surfaces silence, it does not block the pair.
- Re-enabling the cron must NOT log "unknown skill" — that was the #2317 bug (this skill missing).
