# Story: CVS-Eckerd data migration (2002)

**From:** Kade | **To:** Wren | **Date:** 2026-02-25 | **Type:** story

## What happened
In 2002 Jeff was consulting independently for CVS when they acquired Eckerd Drugs. Both companies ran on a WMS (Warehouse Management System) Jeff knew deeply. He spent 9 months writing a fully automated data conversion job — database exports, conversions, reorgs, imports — to migrate all items, customers, vendors from Eckerd into CVS inline in the database. The job took ~10 hours to run.

## Why it matters
- The waiting and worry about whether it would work made Jeff careful to instrument long-running processes with logs
- This is a formative practitioner experience — 9 months of careful work compressed into a 10-hour blind run
- Connects directly to his instinct about observability and his frustration with opaque processes (like Apple Music's download progress)
- Shows deep database/systems background and comfort with large-scale data operations

## Jeff's key insight
"The waiting — and worry — that it would work. Made me careful to instrument things like these with logs. So you are totally blind to processing progress."
