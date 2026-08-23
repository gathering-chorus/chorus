# Morning Summary — 2026-08-23

**HEADLINE:** npm ci has been broken for 75 days — all four test suites are blocked and this needs a decision today.

---

**OPS:** YELLOW (Silas, 2026-08-22)
- 3 yellows, 0 reds, 1 unknown
- Top concern: 8 unused symbols in chorus-hooks (dead code from nudge retirement #2804 — prune before warnings compound)
- Secondary: 25 scripts + 37 plist files use /tmp; need CSC/reaper coverage audit
- Unknown: disk delta — Silas needs to run perf-baseline.sh locally and flag if >2% growth

**QUALITY:** RED (Kade, 2026-08-23)
- 0 tests run — ts-jest preset not found across all 4 suites (directing/clearing, workflow-engine, chorus-sdk, pulse) — **day 73**
- Lint blocked (@eslint/js not found) — **day 75**
- Build: YELLOW — 234 type errors (down 1 from 235, first decrease after 7-day plateau — encouraging)
- Fix: `npm ci` at repo root restores everything. One command. 75 days.

**YESTERDAY:** Active throughput — 10+ cards merged (wren: #3982, #3976; silas: #3986, #3981, #3979, #3977; kade: #3984, #3677, #3922, #3974, #3975, #3978). Notable: silas landed #3986 renaming chorus:deploys → chorus:runsService (10/10 drift bats green).

**TODAY:**
1. **npm ci** — run it or decide to formally defer (but document the decision; 75 days is past the point of drift)
2. Prune chorus-hooks dead code (Wren + Silas, small lift, high signal-to-noise)
3. CSC audit: confirm /tmp lock/log paths are reaper-covered
4. Watch build type-error trend — 234 is the first decrease; keep the pressure on

**BLOCKERS (needs Jeff):**
- `npm ci` at repo root — **75 days unresolved.** No tests, no lint, no coverage data. Either run it or make a call to formally skip the test layer for now.
