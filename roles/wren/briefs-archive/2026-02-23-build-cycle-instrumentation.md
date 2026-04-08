# Brief: Build Cycle Instrumentation — Iteration Drag Is Real

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-02-23
**Card:** #247
**Priority:** P1

## Jeff's Words

"I need to instrument enough data to understand — time to test, time to build and time to deploy. Our test window is getting bigger and bigger and maybe too much depending on where we are in an iteration. Just one of several examples where our iteration decreases due to one of these three things."

## What I'm Seeing

Three forces are silently slowing iteration speed, and we have zero visibility into any of them:

### 1. Test time is growing unchecked
- Started fast. Now 2300+ tests, 1-2 minutes on every push.
- The pre-push hook runs the full suite — blocks all roles.
- Today it corrupted sessions.db (again) by hammering SQLite concurrently with the running app.
- Nobody noticed the growth until it blocked a push for over a minute.

### 2. Deploy time has crept
- Target: <5s. Actual: 22-30s.
- Root cause: `fullSyncAll()` at startup syncs 5,800+ music files to Fuseki.
- Kade fixed the event loop saturation (async + throttling), but the sync still runs.
- Every restart/deploy = 30s where the app is partially available.

### 3. Build time is invisible
- No measurement from "card moved to Now" to "code committed."
- We can't tell if a 2-hour task took 2 hours of building or 1 hour of building + 1 hour fighting infrastructure.
- Contention (three agents, one environment) adds hidden build time that doesn't show up anywhere.

## The Pattern: Iteration Drag

Each cycle gets heavier because these three windows grow unchecked. It's compounding:
- More collections → more tests → longer test runs
- More data → longer sync → longer deploys
- More roles active → more contention → more build friction

Jeff called it exactly right — "depending on where we are in an iteration." Early iterations are fast. Late iterations carry accumulated weight. Without instrumentation we can't see the drag curve or decide where to invest.

## What I'm Proposing (Card #247)

Instrument three clocks with structured events to chorus.log:

| Clock | Start Event | End Event | Current Estimate |
|-------|------------|-----------|-----------------|
| **Build** | `board-ts move <id> Now` | `git commit (card ref)` | Unknown |
| **Test** | `npm run test` starts | Test exit code | 1-2 min (growing) |
| **Deploy** | `app-state.sh deploy` | Health check passes | 22-30s |

Events flow: chorus.log → Promtail → Loki → Prometheus (via recording rules) → Grafana panel.

No new infrastructure. Just timestamps at the right boundaries. The "build health lava lamp" card (#138) was an earlier version of this idea — #247 supersedes it with the full picture.

## Product Implications

This connects to several things in your domain:

1. **Value stream accuracy**: Workflow cycle time measures wall clock, but doesn't decompose into build/test/deploy. A 4-hour cycle time where 2 hours was waiting for tests tells a different story than 4 hours of building.

2. **Test strategy decisions**: When should we run the full suite vs. a focused subset? Right now it's all-or-nothing. With test time data, we can make informed choices — full suite on deploy, fast suite on commit.

3. **Contention as product cost**: Three agents sharing one environment isn't free. The cost shows up as build time inflation. Instrumentation makes it visible so you can factor it into sequencing (e.g., don't run Silas perf tests while Kade is deploying).

4. **Jeff's iteration experience**: He feels the drag before he sees the numbers. The numbers let him (and us) act on it before it compounds.

## Sequencing Suggestion

This is foundational visibility — I'd suggest it goes ahead of new feature work. Without it, we're optimizing blind. The implementation is lightweight (~2 days), reuses existing infrastructure (chorus.log + Loki + Grafana), and the Grafana panel gives Jeff a single place to see all three clocks.

---
*Silas | Architect*
