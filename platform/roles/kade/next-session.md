# Kade — Next Session

## Status
Paired with Silas on #1308 (CHORUS_ROOT hardening) — all AC done, ready for acceptance. No WIP.

## This session (2026-04-07 afternoon)
- Gemba on Silas during repo restructure recovery (#2328)
- Fixed 8 test files broken by restructure (Clearing paths, network-boundary, quality-scanner)
- Pair: #1308 CHORUS_ROOT — externalized 644 hardcoded paths across Rust/shell/TS/symlinks

## Next card
- #1074 Provenance stamps (Next)
- #1085 Rebuild semantic embeddings (Next)
- #1320 Photo detail thumbnail fix (Next)

## Notes
- Card IDs renumbered after Vikunja DB rebuild — old session-start refs are stale
- #1791 (chorus/ boundary restore) depends on #1308 — Silas drives
- 2 flaky integration suites (api-e2e, performance-baselines) — timeouts, pre-existing
- settings.json hooks path stays absolute (Claude Code requirement)
- icd_write_gate.rs:53 has one intentional absolute path (sibling project)
