# Current Work

Last updated: 2026-04-07 16:59 Boston

## WIP
None — #1308 complete, pair ended.

## This Session
- Gemba on Silas (#2328 repo restructure recovery) — watched ~55 min
- Fixed 8 test files broken by restructure (Clearing paths, network-boundary, quality-scanner)
- Pair with Silas on #1308 (CHORUS_ROOT hardening) — 37 min, I drove
  - state_paths.rs: chorus_root() with OnceLock + env var fallback
  - 17 Rust prod files + 29 Rust test files migrated (67 test paths)
  - 34 shell scripts migrated to CHORUS_ROOT variable
  - 5 TypeScript files migrated (api, clearing, board-client)
  - 18 symlinks converted absolute → relative
  - Smoke: cards CLI, chorus-log, hooks all pass
  - Regression: 231/233 suites, 4689/4696 tests (2 pre-existing flaky timeouts)

## Blockers
None

## Queue (renumbered after Vikunja rebuild)
- #1074 Provenance stamps (Next)
- #1085 Embeddings (Next)
- #1320 Photo detail thumbnail fix (Next)
