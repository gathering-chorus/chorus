# DEC-2209: Never Commit Compiled .js/.d.ts Beside Their .ts Source

**Date:** 2026-04-19
**Source:** Kade, during #2209 audit (follow-on from #2201)
**Status:** Active

## Decision

Compiled `.js` and `.d.ts` artifacts must never be committed next to their `.ts` source files in any TypeScript project under `platform/` or `directing/`. They are treated as build output and gitignored in every known TS project's `src/` tree.

## Why

`ts-jest` resolves modules by file-system match order. When a stale `.js` sibling exists next to a `.ts` source, jest instruments the compiled `.js` and silently ignores the `.ts`. Coverage reports and passing tests both become meaningless — the test suite may be exercising stale compiled output rather than current source.

**Incident:** #2201 found 12 stray compiled files in `platform/workflow-engine/src/` that shadowed their `.ts` siblings. Removing them (no test changes) moved workflow-engine coverage from 0 → 95.83% — 61 pre-existing tests had been running silently against stale output.

**Audit:** #2209 scanned the other five TS projects (`platform/api`, `platform/chorus-sdk`, `platform/pulse`, `directing/clearing`, `directing/products/cards`) and found zero strays. The #2201 pattern was isolated — baseline coverage numbers (#2197) were not undercounted — but the recurrence risk is universal.

## Enforcement

`.gitignore` entries for each known TS project block both `src/**/*.js` and `src/**/*.d.ts`. Legitimate compiled output belongs in `dist/`, which is already gitignored.

## Scope

Applies to:
- `platform/api`
- `platform/chorus-sdk`
- `platform/pulse`
- `platform/workflow-engine`
- `directing/clearing`
- `directing/products/cards`

New TS projects must add matching entries when they land.

## Related

- #2201 — discovery (workflow-engine coverage anomaly)
- #2197 — baseline coverage measurement
- #2209 — audit and enforcement
