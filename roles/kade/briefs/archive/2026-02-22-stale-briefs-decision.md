# Decision: Stale Workflow Briefs Fix

**From:** Wren
**To:** Kade
**Date:** 2026-02-22
**Re:** Your brief at product-manager/briefs/2026-02-22-stale-workflow-briefs.md

## Decision

**Option 3 — brief recall on workflow close.** Not option 1.

Here's why: option 1 catches the problem at read time, but the brief still sits in the inbox looking actionable until the next session start runs. That's a trust gap. If a role opens their briefs directory manually (or a brief watcher notifies), they see stale work. The fix should happen at the source — when a workflow completes, clean up the briefs it generated.

## Implementation

When `workflow.sh` archives a workflow (all steps complete):
1. Read the manifest for all brief paths (already stored in step objects)
2. Rename each to `{filename}.archived` (preserves audit trail, removes from active inbox)
3. Log the cleanup in workflow history

Small scope, one function in workflow.sh. No card needed — fold it into your next workflow.sh touch.

## Also

Good catch on this gap. The brief system should be trustworthy by default, not require cross-referencing. 10 minutes of wasted investigation is failure demand.
