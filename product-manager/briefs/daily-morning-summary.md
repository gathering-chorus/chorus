# Daily Morning Summary — 2026-07-22

**HEADLINE:** Quality layer enters week 7 dark — npm ci at day 43 still unowned; CLAUDE.md fragments now 10d stale with Wren's refresh action item unresolved from yesterday.

**OPS:** RED (2 REDs, 3 YELLOWs) — review from 2026-07-21
- RED: Stale WIP — #1759/#1791 at 106d no commits; Wren backlog 18d; #3607 unescalated
- RED: CSC compliance — 36 `platform/scripts/*.sh` /tmp refs; no movement
- YELLOW: CLAUDE.md fragments — 10d stale (3d over threshold); Wren flagged yesterday, not done
- YELLOW: Domain context — chorus/infra/music/seeds 10d stale; chorus domain critical (cards shipped Jul 17)
- YELLOW: LaunchAgent /tmp — 17+2 plists; carry
- GREEN: Hooks cargo check clean; repo dirty state clean

**QUALITY:** RED — day 43 lint; day 41 tests; 154 type errors day 15 (no change)
- 0 tests: all 4 suites (clearing, workflow-engine, chorus-sdk, pulse) blocked; ts-jest preset missing
- 0 lint: @eslint/js not found; same root cause
- `npm ci` at repo root unblocks everything — one command, 43 days without an owner
- No new regressions; counters incremented only

**YESTERDAY (07-21):** Zero cards shipped; Silas filed ops + quality reviews only
- No resolutions; all stale counters ticked +1d; Wren claudemd action item not closed

**TODAY:**
1. **Jeff → `npm ci`:** Day 43; assign ownership today or close these items permanently
2. **Wren (this session):** Refresh `designing/claudemd/` — 10d stale, 3d over threshold, second day flagged
3. **Wren (this session):** domain-context-chorus.md and music/seeds — chorus critical, cards shipped 5d ago
4. **Jeff/Wren:** Close or archive #1759/#1791 — 106d WIP, board noise; escalate #3607
5. **Silas:** CSC July card for `platform/scripts/*.sh` /tmp migration; nightly baseline JSON to `data/`

**BLOCKERS (needs Jeff):**
- **`npm ci` day 43** — quality layer fully dark; no owner; now entering week 7
- **#1759/#1791 day 106** — stale WIP not self-resolving; must close or archive
