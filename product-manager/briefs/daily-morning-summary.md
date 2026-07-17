# Daily Morning Summary — 2026-07-17

**HEADLINE:** #1759 and #1791 cross the 100d stale milestone today — close or commit is overdue — and quality tooling hits day 36 dark with `npm ci` still unowned.

**OPS:** RED (2 REDs carry, 3 YELLOWs)
- RED: Stale WIP — #1759 (OWL entity model) + #1791 (Restore chorus product boundary) **cross 100d today**; close or re-groom required
- RED: CSC compliance — 36 sh-only `/tmp/` refs in `platform/scripts/*.sh`; July scoped card open but no progress
- YELLOW: CLAUDE.md fragments — 8d stale (corrected from yesterday's 12d; still 1d over threshold); Wren audit outstanding
- YELLOW: Domain context — chorus/infra/music/seeds at 8d (1d over); chorus shipped 5+ cards this week with no refresh
- YELLOW: LaunchAgent /tmp — 17 plists unchanged; migration card open
- GREEN: Hooks cargo check clean; git state clean

**QUALITY:** RED — day 36 test blackout, day 38 lint blackout; 154 type errors day 10 (no change)
- 0 tests: all 4 suites blocked (clearing, workflow-engine, chorus-sdk, pulse) — ts-jest preset missing
- 0 lint: @eslint/js not found; same root cause
- No new regressions; type error count held flat at 154
- `npm ci` at repo root unblocks everything — still has no owner, now day 36

**YESTERDAY (07-16):** No cards shipped — only review/summary commits in log
- No card activity visible in gathering-team; team produced daily reviews only

**TODAY:**
1. **Jeff/Wren:** Decide on #1759 + #1791 — 100d milestone crossed; close them or assign active work
2. **Assign `npm ci`** — day 36; one command unblocks tests, lint, and build across all packages
3. **Wren:** Audit CLAUDE.md fragments (`designing/claudemd/`) for drift (8d, escalating)
4. **Silas:** Refresh chorus/infra domain context (8d stale despite active shipping)
5. **Wren:** Refresh music/seeds domain context (same 8d threshold breach)

**BLOCKERS (needs Jeff):**
- #1759 + #1791 at **100d WIP** — longest-stale cards on the board; close or commit today
- `npm ci` unrun **day 36** — entire quality signal dark; who owns the fix?
