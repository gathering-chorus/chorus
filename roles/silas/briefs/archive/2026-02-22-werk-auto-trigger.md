# Brief: Werk Auto-Trigger — C#52

**From**: Wren | **To**: Silas | **Date**: 2026-02-22
**Card**: C#52 (Chorus board)
**Priority**: P1

## What

When a card moves to Now via `board-ts move <id> Now`, automatically create a workflow manifest via `workflow-ts`. Jeff's goal: move card to Now → system takes over → role picks up → flows through value stream.

## Current State

- `board-ts cmdMove()` in `messages/board-client/src/cli.ts` (line ~213) already emits `card_moved` event to chorus.log with `{card_id, to, board}`
- `workflow-ts create` in `messages/workflow-engine/` already creates WF-NNN.json manifests with steps, roles, handoff briefs
- **Missing wire**: no code connects the card_moved event to workflow creation

## Proposed Change

In `cmdMove()` (cli.ts), after the existing `logEvent('card_moved', ...)` call:

1. If `to === 'Now'`, invoke the workflow engine to create a manifest for that card
2. Workflow source field: `"board:card-moved"`
3. Steps derived from card metadata (owner → first step role, priority → step urgency)
4. Log the workflow creation as a chorus event

## Architecture Questions for Silas

1. **Import vs shell-out?** Both are TypeScript. Importing `WorkflowEngine` directly from `workflow-engine/dist/` avoids subprocess overhead but creates a dependency. Shell-out to `workflow-ts create` is simpler but slower. Recommendation: import directly.

2. **Step generation**: How should we derive workflow steps from a card? Options:
   - A) Card owner = first step, then standard review/verify steps
   - B) Template library keyed by card domain/type
   - C) Single default template (owner builds → reviewer verifies) as v1

3. **Both boards?** Should this trigger on Gathering board moves, Chorus board moves, or both?

## Constraints

- Must not break existing `board-ts move` behavior
- Must handle the case where a workflow already exists for a card (skip, don't duplicate)
- Should work silently — signal-not-narrate (DEC-035). One line: "Workflow WF-NNN created for #52"

## What I Need Back

Your architecture call on questions 1-3, or just build it if the path is clear. Jeff wants this wired — it's the piece flow nerve.
