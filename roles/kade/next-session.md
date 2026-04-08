# Kade — Next Session

## Status
No WIP. #1794 and #1801 shipped and accepted.

## This session (2026-04-08)
- **#1794** — Fix test suite from #1791 restructure. Pair with Wren (62 min). 390 Rust + 106 BDD green. --dry-run on nudge/chat. 12 Rust production hooks fixed (dead paths). Restarted com.chorus.hooks server.
- **#1801** — Rename board-client to cards. Pair with Wren (20 min). Directory, package, CLI, 4 docs, 20 test failures fixed. Cards 20/20 green.
- Domain map v2 artifact: 40 domains across 8 layers with quality overlay
- Team retro on reliability (28% correction rate). Repo structure audit. Product ownership defined.
- Gemba on Silas: #1807 product inventory, #1809 demo gate to shell scripts

## Key discoveries
- chorus-hooks SERVER binary must restart after rebuild (shim forwards via socket)
- 25 domain doc pages missing from doc-catalog (stale #1791 paths)
- git-queue.sh had mangled CHORUS_LOG string — fixed
- API returns 25 domains, reference model names 40 — 15 undeclared
- 27 of 40 domains have zero tests

## Next card
- #1800 — Board test isolation (P1, mine). Tests hit live Vikunja.
- #1619 — Provenance stamps (Next)
- #1630 — Rebuild semantic embeddings (Next)
- #1865 — Photo detail thumbnail fix (Next)

## Notes
- Product ownership: I own Clearing + Cards code
- Sexuality-player on Bedroom needs plist log path fix
- 40 @wip BDD scenarios need step defs (Wren carding)
- Inverse Conway is the operating model — team shape drives code shape
