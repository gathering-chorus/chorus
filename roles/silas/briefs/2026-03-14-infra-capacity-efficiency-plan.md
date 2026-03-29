# Brief: Infrastructure Capacity & Efficiency Plan

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-14
**Priority:** High — Jeff's direction

## Context

Yesterday was bumpy. Jeff wants a plan covering **infrastructure capacity and efficiency** across both Macs. Not a quick fix — a real plan.

## What Jeff Wants

A comprehensive assessment and plan covering:

1. **Capacity** — where are we tight? Disk (Library at 15.8GB free of 2TB), RAM (Library 16GB with Docker services), network, CPU. What's the headroom after #1375 migration completes?

2. **Efficiency** — what's running that shouldn't be? What's over-provisioned? What's under-monitored? The mailhog/WebVOWL cleanup (#1376) was a good example — what else is like that?

3. **Stability** — #1380 (app health hangs 2-3x/day) is the biggest pain point. Where else are we fragile?

4. **Forward plan** — what does the infrastructure need to support the next 3 months of product work (more harvesters, more data in Fuseki, potentially more services)?

## Deliverable

A plan document — not a card dump. Silas's architectural read on the state of the platform, with prioritized recommendations. This feeds into product prioritization directly.

## Related Cards

- #1375 (WIP) — Navidrome + WordPress migration to Bedroom
- #1380 (Next, P1) — App health root cause
- #1384 (Later) — Standardize disk/memory reporting
- #1378 (Done) — Host node-exporter for APFS alerting

## Constraints

- Two Mac minis, zero redundancy, no cloud
- Every new service competes for the same 16GB on Library
- Disk reporting must use `diskutil info /` not `df`/`du` (APFS)
