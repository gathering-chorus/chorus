# Daily Morning Summary — 2026-07-23

**HEADLINE:** 4 cards shipped yesterday (Fuseki membrane clean, coupling ledger at 100); quality layer enters day 44 dark with `npm ci` still unowned.

**OPS:** RED (carry — last ops review 2026-07-21; no 07-23 update filed)
- RED: CSC — 36 `platform/scripts/*.sh` /tmp refs; no movement
- RED: Stale WIP — #1759/#1791 now ~107d; Wren backlog ~19d; #3607 unescalated
- YELLOW: CLAUDE.md fragments — ~11d stale, 4d over threshold; Wren action item now third day overdue
- YELLOW: Domain context — chorus/infra/music/seeds ~11d stale; chorus critical (Jul 17 cards shipped)
- YELLOW: LaunchAgent /tmp — 17+2 plists; carry | GREEN: hooks clean, repo clean

**QUALITY:** RED — day 44 lint; day 42 tests; 154 type errors day 16 (no change)
- All 4 suites (clearing, workflow-engine, chorus-sdk, pulse) blocked; ts-jest + @eslint/js missing
- `npm ci` at repo root unblocks everything — 44 days unresolved, no owner

**YESTERDAY (07-22):** 4 cards shipped
- **#3611 (silas):** Fuseki creds untangled; edge ownership flipped to gathering; membrane PASS; coupling 102→100
- **#3662/#3663 (kade):** nightly wedge guard hardened; sdk clearTimeout fix; hermetic move-WIP tests
- **#3660 (wren):** owl-api tree_read surface + tests; ontology TTL updated

**TODAY:**
1. **Jeff → `npm ci`:** Day 44; all 4 suites dark; assign owner or close these items permanently
2. **Wren (this session):** Refresh CLAUDE.md fragments — 11d stale, 4d over threshold, third day flagged
3. **Silas:** domain-context-chorus.md — ~11d stale, chorus critical; nightly baseline JSON to `data/`
4. **Jeff/Wren:** Close/archive #1759/#1791 (~107d); escalate #3607 to Jeff

**BLOCKERS (needs Jeff):**
- **`npm ci` day 44** — quality fully dark, week 7, no owner assigned
- **#1759/#1791 ~107d** — stale WIP not self-resolving; board noise; must close or archive
