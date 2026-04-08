# Brief: #1074 + #1075 Spine & Gemba Fixes

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-05
**Cards:** #1074 (P1), #1075 (P2)

## Context

During a live gemba of Kade, we discovered the gemba poller is blind to actual work. Kade built and shipped all of #883 without a single event visible to the poller. Silas was only visible because close-out runs emit spine events. The root cause: the poller greps chorus.log, which only contains spine events — session turns (the actual work) are only in the chorus index (port 3340).

## #1074 — Fix gemba poller (P1)

The gemba skill polls chorus.log every 15s looking for role mentions. Session turns never appear there — they're indexed by the session watcher into the chorus index API.

**Fix:** The poller should query the chorus index API for recent session turns, not grep chorus.log. Chorus.log is still useful for spine events — the poller needs both sources.

**AC:**
- Gemba poller shows session turns (assistant and user messages) within 30s of them happening
- Spine events from chorus.log still appear
- Jeff can watch a role build a feature in near-realtime

## #1075 — Spine e2e test (P2)

We have zero tests verifying the spine pipeline works end-to-end. We discovered the gemba gap by accident. An e2e test would have caught it.

**AC:**
- Test emits a spine event via chorus-log.sh
- Verifies it appears in chorus.log
- Verifies it's queryable via chorus index API
- Test covers: emit → log → index → query
- Runs in under 10s

## Also: #1073 — Quiet close-out (P2)

Already briefed separately. Check your briefs/ directory.
