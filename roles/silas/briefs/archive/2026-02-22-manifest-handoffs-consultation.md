# Consultation: Manifest Handoffs — Technical Input Needed

**From:** Wren
**To:** Silas
**Date:** 2026-02-22
**Card:** C#43
**Workflow:** WF-009 (step 2 is yours after I advance step 1)

## Context

Jeff directed: every handoff between roles must be manifest — sent, received, visible. Silent failures (like the Slack bridge) are unacceptable. I've written a spec at `product-manager/briefs/2026-02-22-manifest-handoffs-spec.md`.

## Questions for You

### 1. Event Pipeline Integration
The handoff log is append-only JSON lines at `messages/logs/handoffs.log`. Should handoff events also flow to Loki via chorus-log.sh? Or is the log file sufficient for V1?

### 2. Grafana Dashboard
Would you add a handoff health panel to the Chorus Activity Dashboard? Metrics: handoffs sent/received/stale per role, average receipt time, longest open handoff.

### 3. Auto-Receipt Detection
I'm considering a PostToolUse hook on Read that detects when a role opens a brief file and auto-marks the handoff as received. Any concerns about:
- Performance (Read hook fires on every file read)
- False positives (reading a brief for reference vs. acting on it)
- Interaction with sensitive-paths-hook.sh

### 4. Chorus Ontology Alignment (C#40)
The handoff registry introduces a new entity type. Does "Handoff" belong in the Chorus ontology you're formalizing? Properties: id, type, from, to, artifact, status, timestamps.

## What I Need
Your technical assessment on each question. Not full designs — feasibility and red flags. I'll build based on your input.

## After Your Review
Advance WF-009 with your response: `workflow-ts advance WF-009 --notes "your assessment"`
