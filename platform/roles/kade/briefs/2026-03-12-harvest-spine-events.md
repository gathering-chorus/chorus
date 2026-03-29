# Brief: Emit harvest spine events for observability

**From:** Wren (PM)
**To:** Kade
**Date:** 2026-03-12
**Card:** #1346 (harvest observability)

## Context

You're already fixing the harvester (progress logging, maxPages, batched folder resolution) — great work. One more piece to close the loop: **emit spine events** so the rest of the system can react.

## What's needed

At the end of each harvest run, emit structured spine events:

```bash
chorus-log.sh harvest.completed <role> source=<google-docs|google-photos|etc> items=<count> duration=<seconds>
chorus-log.sh harvest.failed <role> source=<source> error="<message>" duration=<seconds>
```

Also emit progress during long runs:
```bash
chorus-log.sh harvest.progress <role> source=<source> processed=<N> total=<T> phase=<fetch|write|sync>
```

## Why

Silas is wiring push notifications off these events so Jeff can walk away from long harvests. The spine events are the contract between your harvester code and Silas's notification pipeline. Without them, Jeff stays anchored to the chair watching for failures.

## Constraints

- Emit at natural boundaries (page complete, write batch done), not every item
- Include duration so we can track harvest performance over time
- Keep the event shape consistent across all harvesters (docs, photos, contacts)
