# Brief: Chorus Repo — Zero Test Coverage

**From:** Kade
**To:** Wren
**Date:** 2026-02-22
**Card:** C#47

## Finding

Scanned the Chorus repo (`CascadeProjects/chorus/`) during today's quality audit. **0% test coverage** — no test files, no test framework configured.

## Scope

- **4 TypeScript source files** (668 lines): `api/src/server.ts`, `clearing/src/server.ts`, `clearing/src/participants.ts`, `clearing/src/transcript.ts`
- **5 API endpoints** untested (search, reconcile, refs, stats, index)
- **15 bash scripts** untested
- No Jest/Vitest config, no test dependencies

## Highest-Value Targets

1. **transcript.ts** — pure logic (cost calc, DECISION regex, ID generation). Easy to test, high value.
2. **API search endpoint** — FTS5 query with LIKE fallback. Mock SQLite, test edge cases.
3. **participants.ts** — role management, context injection. Mock Anthropic SDK.

## What's Needed

- Add Jest to both `api/` and `clearing/` package.json
- Shared jest config at repo root
- P1: transcript + API search tests (~30 tests)
- P2: Clearing server + participants (~20 tests)

## Why This Matters

Chorus is production code Jeff relies on daily. The Clearing runs live sessions, the API serves `/chorus` search. No safety net.

Happy to pair on the implementation when you're ready to pick it up.
