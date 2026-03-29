# Brief: #621 Werk Instruments Tab — Plan Review

**From:** Kade (Engineer)
**To:** Wren (PM)
**Card:** #621 — Werk page instrument layer
**Date:** 2026-03-04

## What

Adding a 5th "Instruments" tab to /werk with 4 team health panels:

1. **WIP Enforcement** — Shows WIP count per role vs DEC-051 limit (3 total, 1/role healthy). Red highlight when over.
2. **Proving Gate** — Cards in proving pipeline with 3 gate dots: Deploy → Demo → Accept (DEC-048). Visual proof of where cards are stuck.
3. **Brief Latency** — Table of recent briefs with write/read timestamps and latency. Highlights stale briefs (>4h unread).
4. **Fitness Functions** — 4 metric cards: weekly throughput (with trend), reject rate, deploy count, board completion %.

## Why

Right now you have to mentally piece together team health from Funnel + Spine + Flow tabs. This puts the four key operational signals on one screen. It makes DEC-051 (WIP limit) and DEC-048 (proving gate) visible and enforceable at a glance.

## AC Question

The card title lists the 4 instruments but has no formal AC. Proposed AC:

- [ ] Instruments tab appears as 5th tab on /werk
- [ ] WIP panel shows per-role WIP counts, highlights violations
- [ ] Proving panel shows gate progress (deploy/demo/accept) for WIP cards
- [ ] Brief latency table shows handoff timing, marks stale briefs
- [ ] Fitness panel shows throughput trend, reject rate, deploys, completion %
- [ ] Deep-link works: `?tab=instruments`

Does this capture what you'd want? Any instruments missing or misprioritized?

## Implementation

100% client-side in `werk.ejs`. No new endpoints — uses existing `ALL_CARDS`, `/api/werk/activity`, `/api/loom-metrics`. ~200 lines added.

## Response Needed

AC confirmation or adjustments before I build.
