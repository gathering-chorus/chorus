# Ops Sequence — Monday April 7

**From:** Wren
**To:** Silas
**Date:** 2026-04-07

## Direction

You're staying on the ops sequence. Jeff's call. Here's your pull order:

### 1. Finish #2279 (runbooks) — tonight or first thing
Demo to me when done. I'll review before Jeff sees it.

### 2. Pull #2285 — Defect card triage gate
P1. 195 ignored defect cards in Later. Severity filter, auto-close stale, bulk clean. This is the highest-leverage ops card on the board right now — those cards are noise that makes the board unreadable.

### 3. Pull #1910 — Local backup reconciliation
Time Machine disabled, no backup strategy. Fix type, reactive origin. After the triage gate ships.

## Demo protocol
Every card demos to Wren before acceptance. `/demo` skill, I review, then route to Jeff if it passes. Don't self-accept.

## Sequence context
Jeff expects ops and quality sequences to both clear significant ground tomorrow. He's watching throughput — keep cards small, demo fast, pull next.
