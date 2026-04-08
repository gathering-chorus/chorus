# Consultation: Manifest Handoffs — Engineering Input Needed

**From:** Wren
**To:** Kade
**Date:** 2026-02-22
**Card:** C#43
**Workflow:** WF-009 (step 3 is yours after Silas completes step 2)

## Context

Jeff directed: every handoff between roles must be manifest — sent, received, visible. I've written a spec at `product-manager/briefs/2026-02-22-manifest-handoffs-spec.md`. Need your engineering perspective before I build.

## Questions for You

### 1. workflow-ts Integration
The new workflow engine (TypeScript, at `messages/workflow-engine/`) already writes handoff briefs on advance. Can the advance flow also append a handoff event to `messages/logs/handoffs.log`? This would be a small addition to `engine.ts`.

### 2. Brief-Write Detection
When any role writes a file to another role's `briefs/` dir, that's a handoff. Options:
- A. Extend write-scrubber-hook.sh to also log handoff events when target is a briefs/ dir
- B. Add a separate PostToolUse hook on Write for briefs detection
- C. Manual logging (caller is responsible)
Which is cleanest?

### 3. session-start.sh Changes
I want to add stale handoff detection to session-start.sh — check for handoffs where status=sent and age > 4 hours. Currently session-start runs in <1 second. Would adding a log file scan add meaningful latency?

### 4. Your Experience
What handoffs have you seen break? Where do briefs get lost? What would have caught the gap earlier? Your practical experience matters more than my theory here.

## What I Need
Your assessment and any alternative approaches. I'll incorporate your feedback and build V1.

## After Your Review
Advance WF-009 with your response: `workflow-ts advance WF-009 --notes "your assessment"`
