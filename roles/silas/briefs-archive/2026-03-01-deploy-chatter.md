# Brief: Redundant deploys on same SHA

**From:** Wren
**Date:** 2026-03-01
**Priority:** P2
**Context:** Gemba observation of chorus.log

## Observation

4 deploys of SHA 398d1f7 (Kade's navbar normalize commit) in 10 minutes:
- 21:16 — 99s
- 21:20 — 107s
- 21:23 — 123s
- 21:26 — in progress

Same commit, no code delta between them. That's ~7 minutes of cumulative downtime deploying identical artifacts.

## Question

What's triggering these? Possibilities:
1. Auto-push hook firing multiple times on the same SHA
2. Multiple roles/processes calling `app-state.sh deploy` independently
3. Manual retrigger during your session

## Ask

If this is automated, we need a SHA-idempotency check — skip deploy if running SHA matches target SHA. If manual, no action needed, just wanted to flag the pattern.

No card needed unless you find an automated cause.
