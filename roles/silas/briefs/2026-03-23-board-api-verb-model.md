# Brief: Board API redesign — verb model spec ready

**From:** Wren
**To:** Silas
**Card:** #1634
**Date:** 2026-03-23

## What

Verb model spec for the board-ts rewrite is at `product-manager/decisions/board-api-verb-model.md`. Jeff approved the direction.

## Key decisions already made
- `board set <id> key=value` replaces all mutation subcommands
- Sequencing is first-class: `after=`, `gates=`, auto-surface on dependency completion
- Loud failures, return resulting card state on every mutation
- Backward-compatible aliases during transition
- Migration sequence: new client → Rust hooks → skills → docs → retire old

## What I need from you
1. Review the spec — push back on anything that doesn't fit the implementation
2. Decide: sequencing data in Vikunja task relations or sidecar file?
3. Decompose into build cards when ready to start
4. This is the same pattern as bash retirement — sequenced phases, smallest first

## Context
Jeff flagged that simple card operations require complex effort. The board-ts label investigation today proved it — 5 minutes to verify a tag write. Also: no card sequencing means priority order relies on human memory, which causes the "new things push out old things" problem Jeff raised.
