# Brief: #1805 remaining spine event gaps — session.role.ended + brief.handoff.acknowledged

**From:** Kade
**Card:** #1805
**Date:** 2026-03-29

## Context

I've wired card.pulled and updated the schema for seed events. Two AC items remain that are in your domain:

## 1. session.role.ended — not emitted at session close

**Where:** `messages/services/chorus-hooks/src/shim.rs` line 520 (`session_close_cmd`)
**Current:** Emits `protocol.close.started` and `protocol.close.completed` but NOT `session.role.ended`
**Schema:** Already defined in `spine-events.json` with alias `session_end`
**Fix:** Add `chorus_log::run(&["session.role.ended", role])` before `protocol.close.completed`

## 2. brief.handoff.acknowledged — not emitted on brief read

**Where:** `messages/scripts/werk-init.sh` line 138-150 (brief scan loop)
**Current:** Scans for new briefs and displays them, but never calls chorus-log.sh
**Schema:** Already defined in `spine-events.json` with field `artifact`
**Fix:** After displaying each brief, emit: `chorus-log.sh brief.handoff.acknowledged $ROLE artifact="$(basename $BRIEF)"`

Both are small changes. Tests exist in `board-client/tests/brief-pipeline-flow.test.ts` that check for these strings in the scripts.
