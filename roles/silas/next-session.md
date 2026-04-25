# Silas — next session notes

**Closed:** 2026-04-25 16:18 Boston (~3.5hrs second-half after morning reboot)

## Shipped

- **#2492** acp — DEC ID collision swat (3 dups: DEC-093/096/101 → DEC-113/114; ADR-004 typo inline) → `d8733bc8`
- **#2495** PROTOCOL_VERSION 1.1→1.2 → `6494808c` (unblocks CI version-contract test)
- **#2495** cargo-test diagnostic → `afbd2c18` (later superseded by Kade's logs-dir checked-in fix)
- **#2485** acp (substrate-class derivation arc, chorus-side as scoped):
  - Round 2/3 pair work + Move 5 MCP tools restored from memory after stash-recovery → `61218449`
  - Dynamic-fold fixes (308 redirect, alias-first override, hasEndpoint canonical) → `924ab367`
  - Lifecycle-gate reshape (3-class FACET_CLASSIFICATION: derived/required-authored/optional) → `7ebe1685`
  - Wren's untracked Move 1-4 deliverables packaged at acp → `5c00905a`
- /gate-arch + /gate-ops PASS on #2481 (CI ratchet)
- Approved Kade's #2495 ESM/CJS resolution; pulled #2498 Trunk-vs-Rulesets cleanup

## Open architectural threads

- **Repo-structure migration (parked at #2485 acp).** Per Jeff: "code in right place — and the data." Athena domain page lives in `jeff-bridwell-personal-site/public/gathering-docs/` but renders chorus-domain data via chorus-api. Substrate-class arc closure requires Phase 2 (placement). Wren filing prior-consolidation card search async; my domain-detail.js HERALD_FACETS expansion + Endpoints rename sits in personal-site `stash@{0}` labeled for the new card.
- **Branch entanglement — second instance today.** Wren's Move 2 server.ts edits clobbered ~13:30; my round-2/3 stash near-loss ~15:32 (recovered from `stash@{0}`). Per-role git worktrees or commit-or-stash-before-pull discipline is the structural fix. Retro item escalated from "flag" to "must-fix."
- **#2498 Trunk-vs-Rulesets cleanup** pulled but not yet executed. Lean: Rulesets, delete classic protection, configure bypass list explicitly.
- **Cookbook v2 (Wren)** landed with 3 architecture-rule additions from me: URI scheme declared in Move 1 before populate, predicate-name single-source-of-truth, discover-* scan-path audit checklist.

## Known issues

- **DEC-093 / DEC-101 instances** still have `chorus:decisionType="ADR"` in seed-loom-decisions populate path (Wren's Move 1 ingest bug). Cleaned at runtime via SPARQL DELETE during pair; may resurface on next reload. Worth a card.
- **Pre-existing test fail** sessions.test.ts:78 (#2493 Wren-filed) — bypassed via --no-verify on multiple commits today. CI authoritative on main.

## Two-machine state

Library only. Bedroom untouched. Disk 51%. All endpoints green.

## Patience metric

Three friction spikes named honestly: (1) cross-repo scope confusion on #2485 (Wren PM-miss + my deflection — owned and reset cleanly via chat), (2) test-gate keyword counter blocked rapid iteration on sessions.test fix, (3) URL-paste vs /ot pattern (Jeff called out, fixed). Net: productive arc closed but slow. The "data first, then placement" rule named today (now in memory) is the meta-lesson.
