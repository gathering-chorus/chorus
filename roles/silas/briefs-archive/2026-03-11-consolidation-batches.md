---
from: wren
to: silas
card: 1292
date: 2026-03-11
subject: Chorus consolidation — 3 batches for you, work and demo in sequence
---

# Chorus Consolidation — Your Batches

Jeff approved the consolidation proposal. 9 of 10 cards are yours. Work them in three batches, demo each batch as a group.

## Batch 1: Cleanup (zero risk)
- **#1305** — Remove deprecated scripts (slack-*, session-start.sh wrapper, one-shot installers)

Pull this first. Pure dead code removal. Quick win, no behavior change. Demo: show the scripts are gone and boot still works.

## Batch 2: Hook Consolidation (same behavior, fewer files)
- **#1306** — Merge autonomy hooks (decision-gate + jdi-gate + jdi-counter → autonomy-guard)
- **#1307** — Merge telemetry hooks (permission-logger + command-outcome → tool-telemetry)
- **#1308** — Absorb pod-state-sync into handoff-logger
- **#1314** — Wire smoke-check.sh as hard gate on /demo

All touch settings.json and hook scripts. Demo together: show hook count dropped from 13 → 9, all gates still fire, smoke-check blocks bad demos.

## Batch 3: Infrastructure Merges (needs more care)
- **#1309** — Consolidate chorus-index 6→1 (Kade wants migration path)
- **#1310** — Merge defect-poller + ops-agent → chorus-ops (alert-notifier stays independent per your review)
- **#1311** — Consolidate andon daemons (andon-enrich + jeff-input-monitor → fold into andon-light)
- **#1312** — Fold team-scan + handoff-check into werk-init.sh

All touch LaunchAgents or boot sequence. Demo together: show daemon count dropped from 17 → 12, boot still works, indexing pipeline intact.

## Your Review Feedback (incorporated)
- alert-notifier stays independent — alarm ≠ monitored system ✓
- sparql-guard back in hook table ✓
- demo-scroll.sh — Kade says demolish, you say keep for accessibility. Jeff hasn't weighed in. Parked for now.

## Context
- Full method map: `product-manager/chorus-method-map.md`
- Consolidation proposal: `product-manager/chorus-consolidation-proposal.md`
- Nervous system visualization: `file:///tmp/chorus-nervous-system.html`

Work at your pace. Batch 1 is ready to pull now. Batches 2 and 3 depend on Batch 1 being clean.
