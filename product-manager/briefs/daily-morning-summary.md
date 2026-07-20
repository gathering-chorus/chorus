# Daily Morning Summary — 2026-07-20

**HEADLINE:** Quality layer dark for 41 days straight — `npm ci` is still unowned, every suite and every lint check remains broken.

**OPS:** RED (2 REDs, 3 YELLOWs)
- RED: Stale WIP — #1759/#1791 at 103d no commits; Wren backlog 15d; #3607 log rotation still broken live
- RED: CSC compliance — 36 `platform/scripts/*.sh` with `/tmp/` refs; no movement
- YELLOW: CLAUDE.md fragments — 11d stale (4d over threshold, escalating); audit overdue
- YELLOW: Domain context — chorus/infra/music/seeds 10d stale; chorus stale despite #3658 shipping
- YELLOW: LaunchAgent /tmp — 17 plists, carry
- GREEN: Hooks cargo check clean; repo dirty state clean

**QUALITY:** RED — day 41 lint blackout; day 39 test blackout; 154 type errors day 13 (no change)
- 0 tests run: all 4 suites (clearing, workflow-engine, chorus-sdk, pulse) blocked; ts-jest preset missing
- 0 lint: @eslint/js not found; same root cause
- `npm ci` at repo root unblocks everything — 41 days without an owner

**YESTERDAY (07-19):** Zero cards shipped; Silas filed ops + quality reviews only
- All counters ticked up; no regressions; no resolutions

**TODAY:**
1. **Jeff → `npm ci`:** Day 41; one command unblocks all tests, lint, and build — assign an owner
2. **Jeff → #3607:** Log rotation fix is staged and waiting; chorus.log growing unchecked
3. **Wren:** Audit `designing/claudemd/` — 11d stale, 4d over threshold, escalating daily
4. **Jeff/Wren:** Close or archive #1759/#1791 — 103d WIP, no commits, pure board noise
5. **Silas:** Refresh chorus domain context — stale despite #3658 shipping last week

**BLOCKERS (needs Jeff):**
- **`npm ci` day 41** — six weeks with the entire quality layer dark; needs an owner today
- **#3607** — chorus.log log rotation broken live; fix staged, approval pending
