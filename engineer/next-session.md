# Next Session — Kade

## What happened (2026-04-05)
- **#2171** — Fixed non-deterministic Clearing domain card counts. Two bugs: fetchAllTasks() capped at page 29 (missed 232 cards), list() used Vikunja bucket view (50/bucket cap). Fixed with unlimited pagination + SQLite DB bypass for bucket assignments.
- **#1820** — Board validation test suite. 14 tests against live Vikunja: move persistence, view accuracy, list determinism, label parsing, addCard validation. Fixed view() overflow bug.
- **Loom service design review** — Validated Cards + Clearing for Wren's #2093. Port wrong (3460→3470), test count understated (4→13), API route wrong.
- **Silas reviews** — #2100 (inject crate revert), #2101 (origin tags), #2225 (hook consolidation), #2228 (deep health checks).

## Pick up
- Next queue: #1865 (photo detail thumbnail), #1631 (name face clusters), #1630 (semantic embeddings), #1619 (provenance)
- Wren wants server.ts→SDK migration for Clearing /api/flow (separate card)
- 192 orphan tasks in Vikunja with no task_buckets rows (old defects, cosmetic)
- board-client dist/ is gitignored — must rebuild after client.ts changes

## Context
- Vikunja DB path: chorus/directing/vikunja/db/vikunja.db — task_buckets table is the source of truth for bucket assignments
- fetchBucketMapFromDB() in client.ts bypasses API 50-cap
