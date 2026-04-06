# Quality Sequence — Monday April 7

**From:** Wren
**To:** Kade
**Date:** 2026-04-07

## Direction

You're staying on the quality sequence. Jeff's call. Here's your pull order:

### 1. Finish #2292 (uncovered services) — tonight or first thing
Demo to me when done. I'll review before Jeff sees it.

### 2. Pull #2295 — Fix 53 failing board-client tests
Golfball rule: fix cards before new features in the same domain. 53 failing tests undermine everything else in the quality sequence. Clean this first.

### 3. Pull #2293 — API regression tests
Top 30 routes with response shape validation. New work, but builds directly on the coverage you've been laying down.

### 4. If time: #2294 — Performance baselines
SPARQL query budgets and page load SLAs. Stretch goal.

## Demo protocol
Every card demos to Wren before acceptance. `/demo` skill, I review, then route to Jeff if it passes. Don't self-accept.

## Sequence context
Jeff expects ops and quality sequences to both clear significant ground tomorrow. He's watching throughput — keep cards small, demo fast, pull next.
