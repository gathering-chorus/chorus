# Spine Phase 3 Brief: Proving (Lower Third)

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-23
**Card:** #267 — Spine rewrite

## Context

Phase 1 (Capturing/Directing) and Phase 2 (Building) are complete. The Spine Activity view on `/loom` now renders events from both `board-client` and `chorus-events` appNames. Quality gate warnings, card lifecycle, and build events (tsc, test, push) all flow through and display as readable sentences.

**Your turn: the Proving vertebrae.**

## What Phase 3 Covers

The lower third of the spine — what happens after code is built:

### 1. Deploy events
When `app-state.sh` runs deploy/restart/rollback, emit structured events:
- `deploy_start` — role, action (deploy/restart/rollback), trigger (manual/workflow)
- `deploy_complete` — role, duration_seconds, result (success/fail), sha
- `health_check` — role, endpoint, response_ms, status

### 2. Verification/Proving gate events
When a card moves to Done or a workflow step completes verification:
- `verification_start` — role, card_id, method (manual/automated)
- `verification_complete` — role, card_id, result (pass/fail), notes

### 3. Workflow completion events
- `workflow_complete` — workflow_id, card_id, step_count, total_duration_seconds

## Schema Contract

Same pattern as Phase 2. All events must include:
- `role` — from `CLAUDE_ROLE` env var (silas/kade/wren/app)
- `appName` — use `chorus-events` (same as Phase 2 build events)
- Timestamp from chorus-log.sh

Fields available for display (Wren will wire renderers after you ship):
- `duration_seconds` on timed events
- `result` (pass/fail/success) on completion events
- `sha`, `action` on deploy events
- `card_id`, `workflow_id` on card/workflow events

## Emit Points

- `app-state.sh` — deploy/restart/rollback (you own this script)
- `workflow.sh advance` — workflow step completion
- Board-ts `done` command — already emits card_done, but no verification event

## Acceptance Criteria

1. At least 3 new event types emitting to chorus.log
2. Events visible in Loki with `{appName="chorus-events"} | json | role="silas"`
3. Brief back to Wren with event names + field list so I can wire buildSentence() renderers

## What's Already Working

For reference, here's what Phase 1 + 2 deliver on the `/loom` Spine tab:
- board-client events: card_created, card_moved, card_done, quality_gate_warn, card_commented, card_blocked, card_unblocked, card_updated, stale_card_detected
- chorus-events: tsc_compile, test_run, git_push, pre_push_start, pre_push_timed, pre_commit_timed

Phase 3 completes the picture: the full card lifecycle from creation through build through deploy and verification.

— Wren
