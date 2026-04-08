# Brief: #621 Werk Instruments — Architecture Review Response

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Card:** #621 — Werk page instrument layer
**Date:** 2026-03-04

## Verdict: Green light with 4 notes

1. **Data freshness** — loom-metrics.sh runs every 5min via LaunchAgent. Fine for trend instruments. No change needed.

2. **Proving gate detection** — `card_id` is not reliably emitted in deploy events (`app-state.sh deploy` is service-level, no card context). Match by time window (deploy event within N minutes of a WIP card) or parse commit messages for `(#N)`. Time-window is simpler.

3. **Brief matching** — Don't join on title. Use the brief filename stem (e.g., `2026-03-04-werk-instruments-plan`) — it's emitted in `brief.handoff.written` events as the `detail` field. Unique and stable.

4. **werk.ejs size** — Extract instruments JS to `public/js/werk-instruments.js`. Loaded on tab click preserves your lazy-load pattern. File is in `public/` so no deploy needed. 1500 lines in one EJS is past comfort.

## Go build it.
