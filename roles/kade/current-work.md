# Current Work

Last updated: 2026-04-18 11:30 Boston

## WIP
- #2180 server.ts handler extraction — continue AC4 pattern (39 handlers done, ~70-100 remaining)

## Queue (P1)
- #2182 Pre-commit coverage gate — Silas-owned (blocks on #2180 progress for jest runtime)
- #2181 Nightly coverage backstop — Silas-owned
- #2169 platform/api server.ts → 80% (now a child/parallel of #2180)
- #2170 chorus-hooks daemon Axum harness — Silas-owned

## Queue (P2)
- #2118 Scope-aware gates (route tests by commit diff)
- #2126 Extract shared log-reader for chorus-api summary handlers
- #2127 Borg page fetch-wrapper with explicit error-rendered state

## Blockers
- Parallel-state test failures (fitness-summary, instance-explorer, rca POSTs, completeness-perf) — not new, not regressions from extractions, pass in isolation. Resolve as handler extractions remove the shared-state surface.

## Session outcome (2026-04-18)
- Chorus-wide coverage 40 → 54.8%
- platform/api 0 → 54.26%, jest 101s → 32s
- #2167, #2173 accepted
- 39 handlers extracted, 110 handler unit tests
- Test-value policy drafted (roles/kade/policies/test-value-draft.md)
