# Brief: Vision Pages — Awareness

**From**: Wren (PM)
**To**: Kade (Engineer)
**Date**: 2026-02-26
**Re**: Three new static pages in public/, revenue strategy context

## Context

Jeff articulated his revenue strategy this morning. Three containers, three static HTML pages now in `jeff-bridwell-personal-site/public/`:

- `gathering-chorus.html` — Gathering + Chorus as shareable platform
- `lightlife.html` — Light Life Urban Gardens (updated, deepened)
- `chorus-consulting.html` — Local tech consulting business

## What This Means for You

1. **DEC-057 (product maturity threshold)** is now in effect. Jeff wants the system good enough to share — open source or with consulting clients. The bar for "done" on core-tier work is now: shipped + hardened + documented. "Could someone else run this?" is the test.

2. **Tiered rigor model**: Core (harvest pipelines, Chorus surfaces, primary flows) gets full assurance. Enduring (stable, low-volume) gets light checks. Tactical (one-time) ships and moves on.

3. **These pages are static HTML** — no build step, no server rendering. They live in `public/` and are served by Express static middleware. No code changes needed from you right now.

4. **Future implication**: When consulting clients use the system, the app needs to be multi-tenant or at minimum deployable as a clean instance. That's not today's work, but it's on the horizon.

## No Action Needed
Awareness only. Continue with your current queue (#395 gallery convergence, #377 sexuality pipeline). The maturity threshold shapes how we assess "done" going forward.

## Related
- DEC-057 in `product-manager/decisions.md`
- #3 — Vision refinement (Wren, WIP)
- #92 — Revenue strategy (updated)
