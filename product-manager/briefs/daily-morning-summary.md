# Daily Morning Summary — 2026-07-21

**HEADLINE:** Quality layer dark for 42 days — `npm ci` still unowned; CLAUDE.md fragments now 9d stale and Wren is on the hook to refresh today.

**OPS:** RED (2 REDs, 3 YELLOWs)
- RED: Stale WIP — #1759/#1791 at 104d no commits; Wren backlog 16d; escalate #3607
- RED: CSC compliance — 36 `platform/scripts/*.sh` with `/tmp/` refs; carry, no movement
- YELLOW: CLAUDE.md fragments — 9d stale (2d over threshold); ops brief flags Wren to fix this session
- YELLOW: Domain context — chorus/infra/music/seeds 9d stale; chorus domain stale despite #3661/#3657 shipping Jul 17; music/seeds on Wren
- YELLOW: LaunchAgent /tmp — 17+2 plists; carry
- GREEN: Hooks cargo check clean; repo dirty state clean

**QUALITY:** RED — day 42 lint blackout; day 40 test blackout; 154 type errors day 14 (no change)
- 0 tests run: all 4 suites (clearing, workflow-engine, chorus-sdk, pulse) blocked; ts-jest preset missing
- 0 lint: @eslint/js not found; same root cause as tests
- `npm ci` at repo root unblocks everything — 42 days without an owner

**YESTERDAY (07-20):** Zero cards shipped; Silas filed ops + quality reviews only
- All counters ticked up; claudemd stale count corrected (was cited as 11d, git confirms 9d from Jul 12); no resolutions

**TODAY:**
1. **Jeff → `npm ci`:** Day 42; one command unblocks all tests, lint, and build — assign an owner today
2. **Wren (this session):** Refresh `designing/claudemd/` — 9d stale, ops brief flagged, 2d over threshold
3. **Wren (this session):** Refresh music/seeds domain context and domain-context-chorus.md
4. **Jeff/Wren:** Close or archive #1759/#1791 — 104d WIP, no commits, pure board noise; escalate #3607
5. **Silas:** Continue CSC July card for `platform/scripts/*.sh` /tmp migration

**BLOCKERS (needs Jeff):**
- **`npm ci` day 42** — six weeks with the entire quality layer dark; no owner identified
- **#1759/#1791 day 104** — stale WIP must close or archive; not self-resolving
