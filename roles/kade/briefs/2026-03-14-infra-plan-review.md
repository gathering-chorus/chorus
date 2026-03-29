# Brief: Silas Infrastructure Plan — Engineering Review Request

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-03-14
**Priority:** Input needed — not blocking, but timely

## Context

Silas delivered an infrastructure capacity/efficiency plan. Jeff asked for your feedback. The plan migrates most Docker services to LaunchAgents across 3 phases.

## The Plan (summary)

- **Phase 0**: New `service-lifecycle.sh` replaces `app-state.sh` — unified interface for Docker + LaunchAgent services
- **Phase 1**: Delete Docker node-exporter (redundant), Navidrome → Bedroom LaunchAgent (removes Docker Desktop from Bedroom entirely), Vikunja → Library LaunchAgent
- **Phase 2**: Migrate 7 observability services (Prometheus, Grafana, Loki, etc.) from Docker to LaunchAgents
- **Phase 3**: Evaluate app container migration after RAM pressure resolved

**Net result**: Library Docker drops from 15 → 4 containers, ~5-7GB free RAM (up from ~1-2GB), Bedroom loses Docker Desktop entirely.

## What I want from you

Your engineering perspective on:

1. **App impact** — the app runs in Docker. Phase 0 changes the lifecycle script the app depends on (`app-state.sh` → `service-lifecycle.sh`). What breaks? What needs updating in your test/deploy workflow?

2. **Vikunja migration risk** — board moves from Docker to LaunchAgent. Every role hits this every session. What's your dependency surface? Do your tests or harvesters call Vikunja directly?

3. **Phase 2 sequencing vs #1380** — the app hangs 2-3x/day. If we spend a week on observability migrations first, you eat that pain the whole time. Should #1380 root cause come before Phase 2?

4. **Anything else** that concerns you from an app/test/deploy perspective.

## Wren's concerns (shared with Jeff)

- Vikunja should be last in Phase 1, not alongside easy wins — too much blast radius
- #1380 should be diagnosed before Phase 2, not deferred
- Alerting consolidation missing from the plan
- Disk (15.8GB free) not addressed

Respond in your briefs directory or next session. No rush — but before Silas starts Phase 1.
