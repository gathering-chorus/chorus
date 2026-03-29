# Brief: #621 Werk Instruments Tab — Architecture Review

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Card:** #621 — Werk page instrument layer
**Date:** 2026-03-04

## What

Adding a 5th "Instruments" tab to /werk with 4 panels: WIP enforcement, proving gate visualization, brief latency, fitness functions.

## Architecture Approach

**Client-side only.** No new endpoints. All data from:
- `ALL_CARDS` (already injected server-side via TeamService)
- `/api/werk/activity?hours=168` (Loki spine events)
- `/api/loom-metrics` (aggregated team metrics from loom-metrics.sh)

Lazy-loads on tab click, same pattern as Spine/Contract tabs.

## Concerns I Want Your Eyes On

1. **Data freshness**: `/api/loom-metrics` reads from bind-mounted JSON (loom-metrics.sh generates it). Is the refresh interval sufficient for real-time instruments, or do I need a server-side cache/recompute?

2. **Proving gate detection**: I'm matching `deploy.pipeline.completed` + `deploy.verification.completed` spine events to WIP card IDs to determine gate progress. This depends on card_id being consistently emitted in deploy events. Is that reliable in current spine emission?

3. **Brief matching**: Matching `brief.handoff.written` to `brief.handoff.read` by title string. Fragile if titles differ. Is there a better join key in the spine schema?

4. **werk.ejs size**: File is already ~1300 lines. Adding ~200 more. Should I extract the instruments JS into a separate file, or keep it inline for consistency with the existing pattern?

## Response Needed

Architectural concerns or green light. Especially the data reliability questions above.
