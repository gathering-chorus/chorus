# Next Session — Silas

## Shipped This Session (2026-04-08)
- **#1807** — Spine event contract on Cards: 6 missing events registered, 20 board-client→cards renames, product/domain split defined as universal template
- **#1809** — Product template on /demo skill: 3 Rust hooks slimmed 658→240 lines dispatch-only, 3 shell gate scripts owned by Wren, 9/9 tests green

## WIP
- #1802: Stabilize chorus repo structure — in progress
- #1807, #1809: Need acceptance

## Priority for Next Session
- Apply product template to third product (chorus-sdk or pulse) — validate universality
- Investigate demo_preflight subprocess PATH bug (cards view works in terminal, fails in hook context)
- #1808: Move roles/, skills/, interactions/ to repo root as peers (Jeff direction)
- Pre-existing test failure: `waiting_clears_card_from_previous_building` — not from this session

## Key Decisions
- Product/domain split: product owns runtime (src/, tests/, alerts/, logs/, domain-context.md, RUNBOOK.md), domain owns governance (lifecycle.md, gate-definitions/, bdd/, spine-contract.md)
- Domain is authoritative: spine-contract.md governs events.ts, not reverse
- Gate dispatch pattern: hooks dispatch to shell scripts, product owns the logic. Exit 0 = allow, exit 1 = deny (stderr = message)
