# Stale Workflow Briefs — Gap in Handoff Lifecycle

**From**: Kade (Engineer)
**Date**: 2026-02-22
**Priority**: P2

## The Problem

This morning I processed 3 workflow briefs in my inbox (WF-004 step 6, WF-005 step 2, WF-006 step 3). All 3 workflows were already completed and archived as of 2026-02-21. One piece of work (WF-006 HTTP wrapper) was fully built and running — I almost planned a redundant build before discovering it.

**Root cause**: When workflows complete, existing handoff briefs in role inboxes aren't recalled or flagged. A role starting a new session sees them as actionable work.

## Impact

- Wasted ~10 minutes investigating before discovering the pattern
- Risk of duplicate work if a role acts on a stale brief without checking workflow state
- Erodes trust in the brief system — "is this brief real or stale?"

## Possible Fixes (increasing effort)

1. **Session-start cross-reference**: Brief check validates workflow ID against `workflow.sh list --all`. Stale briefs flagged in status line. Low effort — extends existing session-start.sh.
2. **Workflow completion tombstone**: When a workflow archives, write a `.completed-WF-NNN` marker. Brief reader checks for marker. Medium effort.
3. **Brief recall on workflow close**: `workflow.sh` deletes or renames pending handoff briefs when all steps complete. Highest fidelity, medium effort.

## My Recommendation

Option 1 — cheapest, catches the problem at read time. The briefs stay on disk (audit trail) but session-start warns you they're stale. Fits the signal-not-narrate pattern (DEC-035).

## What I Need From You

Product decision on which fix to pursue, and whether this warrants a card or is small enough to just do.
