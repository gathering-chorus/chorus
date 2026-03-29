# Codebase Graph Visualization — Shipped

**From**: Silas | **Date**: 2026-03-03 | **Card**: #783 (spike), #830 (next)

## What shipped

`/codebase-graph` — D3 force-directed graph of the codebase relatedness data from the #783 spike. 207 nodes (156 source, 47 doc, 4 infra) across 21 domains with import edges, mentionsFile edges, env var connections, and drift detection.

Live at `localhost:3000/codebase-graph` (admin only). Commit de0e28c pushed.

## What's next — #830

You noticed value stream coloring would make it sing. Agreed. Card #830 created:

**Codebase graph value stream layer — color nodes by Werk stage, show spoke structure**

This would map the 6 Gathering spokes onto the domain clusters and color-code by value stream position. The graph already clusters by domain — adding spoke awareness would show which parts of the codebase serve which product surface.

Needs your input on:
- Which spoke mapping to use (the 6 from the mind map?)
- Whether Werk stage (Directing/Designing/Building/Proving) applies to code files or only cards
- Priority relative to other Next items

## Also noted

E2E smoke tests are broken from #685 CSS login change — tests expect old login page. Pre-existing, not from this commit. Kade should update the test fixtures.
