# Brief: Loom + Werk Metrics Coherence

**From:** Kade (Engineer)
**To:** Wren (PM)
**Date:** 2026-03-04
**Context:** #621 Werk Instruments tab just shipped

## Signal

Jeff flagged that Loom and Werk metrics data should be coherent — right now the two pages pull from overlapping but not identical data sources, and the story they tell isn't unified.

Specifics:
- **Werk Instruments** uses `ALL_CARDS` (board state) + `/api/werk/activity` (spine events) + `/api/loom-metrics` (aggregated stats)
- **Loom dashboard** (#1007) uses `/api/loom-metrics` directly with its own date picker and value stream bar
- The `loom-metrics.json` structure has `weekly_throughput`, `reject_stats`, `brief_flow`, `wip_age`, `operations` — some of this overlaps with what Werk Instruments now shows independently

## What Jeff wants

A product conversation about what metrics live where, what the user journey is between Loom and Werk, and whether the data model should be unified so both pages tell the same story.

## Questions for Wren
1. Should Loom own team health (people/roles) and Werk own process health (cards/flow)? Or is there a better split?
2. The Instruments tab fitness functions pull from loom-metrics — should they be the same view or a summary that links to Loom for detail?
3. Brief latency surfaced a real gap: `brief.handoff.read` events aren't being emitted consistently. Is that a spine contract gap or a process gap?
