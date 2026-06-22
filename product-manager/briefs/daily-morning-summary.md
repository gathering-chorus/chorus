# Daily Morning Summary — 2026-06-22

**HEADLINE:** All 4 JS test suites are dark Day 14 — `browserslist@4.28.4` was yanked from npm; one lock file pin unblocks everything.

---

## OPS — YELLOW (Silas, 2026-06-22)
- **RED:** Board snapshot 76 days stale (Apr 7); two WIP cards orphaned: "Framework service design — OWL entity model" and "Restore chorus product boundary" — need refresh or close
- YELLOW: 8 dead-code warnings in `chorus-hooks` — day 20; escalates to RED at next weekly
- YELLOW: LaunchAgent logs to `/tmp/` — day 20; blocked without host access
- YELLOW: CLAUDE.md fragments — 45 of 48 still at Jun 5; 3 shared fragments updated today; role files need regeneration (Wren action)
- YELLOW: Domain context — `domain-context-chorus.md` 17 days stale despite 8+ commits this week (Wren action)
- GREEN: Git clean; latest commit #3551 (Wren, Jun 22)

## QUALITY — RED (Kade, 2026-06-22)
- **All 4 suites BLOCKED** — `browserslist@4.28.4` yanked from npm registry, **Day 14**; 0 tests run
- **Lint BLOCKED** — `@eslint/js` missing at root, **Day 16**
- **Build RED:** 150 type errors — unchanged from yesterday (+1 regression from Jun 21 still unresolved)
- Fix: pin `browserslist` to latest stable in lock file → `npm ci` across all packages; unblocks all 4 suites at once

## YESTERDAY — 7 cards shipped (Jun 21)
- **Wren (3):** #3544, #3548, #3551
- **Silas (2):** #3540, #3550
- **Kade (2):** #3190 (unblocked by Silas #3546 phantom fix), #2818

## TODAY — Recommended priorities
1. **Fix `browserslist` lock file (P0)** — Day 14 blocker; pin to latest stable, run `npm ci` root + all packages; 30-min fix that unblocks all quality signal
2. **Build regression (P1)** — 150 type errors with an unowned +1 from Jun 21; assign before it compounds
3. **Board snapshot refresh (P1)** — 76 days blind; two cards may be long-closed
4. **CLAUDE.md regeneration (P2)** — Wren: run claudemd pipeline; 3 fragments updated today
5. **Domain context refresh (P2)** — Wren: update `domain-context-chorus.md` to reflect CI and worktree changes

## BLOCKERS — Needs Jeff
- **Quality dark Day 14 (RED):** All JS tests blocked by yanked npm package. Lock file update is the fix — who owns it?
- **Board state unknown (RED):** 76-day-old snapshot makes WIP invisible. Refresh or grant host access today.
