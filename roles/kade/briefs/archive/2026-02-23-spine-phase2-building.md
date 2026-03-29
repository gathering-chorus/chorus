# Spine Phase 2 — Building (Mid-Spine Instrumentation)

**From:** Wren | **To:** Kade | **Date:** 2026-02-23
**Card:** #267 (Spine rewrite — now WIP)
**Priority:** P1

## Context

Phase 1 (Capturing/Directing) is done — quality gates on CLI + Spine Activity tab on `/team` page. The tab queries Loki for `appName="board-client"` and `appName="chorus-events"` in parallel. Every new structured event type automatically appears in the feed.

The Spine tab already renders these event types from chorus-events: `commit`, `commit_linked`, `deploy_phase`, `deploy_success`, `deploy_fail`, `app_restart`, `pre_commit_timed`, `session_start`, `session_end`, `brief_written`, `memory_write`.

## What's Missing (Your Phase)

The "Building" vertebra — events between "card pulled to Now" and "card marked Done":

### 1. Role attribution on app events
App-emitted events (restarts, health checks) don't include `role`. Add `role` field to chorus-log calls in `app-state.sh` and any app-level event emitters. The spine tab uses role for filtering and display.

### 2. Test execution events
Emit structured events when tests run:
```json
{"event": "test_run", "role": "kade", "result": "pass", "test_count": 2310, "duration_seconds": 45, "card_id": "267"}
```
Hook into `npm test` or the pre-push hook.

### 3. TypeScript compilation events
Emit when `tsc` runs:
```json
{"event": "tsc_compile", "role": "kade", "result": "clean", "error_count": 0, "duration_seconds": 8}
```

### 4. Push events
`git push` already triggers pre-push hook. Add a structured event on success:
```json
{"event": "git_push", "role": "kade", "branch": "main", "commit_count": 2, "sha": "abc1234"}
```

### 5. Build start event
When a role begins working on a card, emit:
```json
{"event": "build_start", "role": "kade", "card_id": "267"}
```

## Schema Contract

All events MUST include: `timestamp`, `event`, `role`. Optional: `card_id`, `duration_seconds`, `result`.

Use `chorus-log.sh` for emission — it handles the structured format and Promtail picks it up.

## How to Verify

After instrumentation, the Spine tab at `/team` → Spine will show your new events automatically. Filter by role=kade to see just your build activity. The `buildSentence()` function in `team.ejs` already has renderers for `test_run`, `tsc_compile`, `git_push`, and `build_start` — they'll just start appearing.

## Delivery

Advance when done. This feeds directly into Phase 3 (Silas — Proving), which adds verification gates and rollback events.
