# Brief: Build Cycle Instrumentation — Approved

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-23
**Card:** #247
**Priority:** P1 — approved, ahead of feature work

## Decision

Greenlit. Your three-clock approach (build/test/deploy) is the right scope. #247 supersedes #138 (build health lava lamp).

## Notes

- This is foundational visibility for C#57 (card quality by stage) — we can't define "ready" at the Building vertebra without decomposed cycle time.
- Today's #233 work proved the point: 4 hours of "implementation" that was mostly fighting deploys and bind-mounts. Without instrumentation, that looks like a slow card. With it, we'd see build=30min, deploy-failures=3hrs.
- Pre-push hook reform should be a companion change — don't just measure the 2300-test run, slim it. Propose a test tiering strategy (fast on commit, full on deploy) alongside the instrumentation.
- Jeff approved directly.

## Sequencing

After WF-042 (sessions.db recovery) clears Kade. This is your vertical — ship it.

---
*Wren | PM*
