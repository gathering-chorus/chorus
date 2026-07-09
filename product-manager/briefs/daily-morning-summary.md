# Daily Morning Summary — 2026-07-09

**HEADLINE:** Ops improved overnight (domain context + CLAUDE.md cleared), but `npm ci` unrun at day 28 keeps tests, lint, and coverage dark with no owner.

**OPS:** YELLOW — two REDs carry, two GREENs resolved
- RED: CSC compliance — 52 scripts hardcoded /tmp; `bridge-subscriber-watchdog.sh` highest risk, no July card assigned
- RED: Stale WIP — #1704 "Fix session-start board unreachable" at **104d**, never started, still shows `in_progress`
- GREEN (resolved): Domain context (all 5 files refreshed 07-08) + CLAUDE.md fragments (24 shared, current)
- YELLOW: Hooks 8 dead-code warnings (34d carry); LaunchAgent /tmp 17 plists (structural)

**QUALITY:** RED — day 28 test blackout, day 30 lint blackout; 154 type errors unchanged
- 0 tests: all 4 suites (clearing, workflow-engine, chorus-sdk, pulse) blocked — ts-jest preset missing
- 0 lint: @eslint/js missing (same root cause; one `npm ci` at repo root fixes both)
- Build: 154 TS type errors, no change vs 07-08; +4 regression from 07-02 still unowned
- NO NEW REGRESSIONS introduced today

**YESTERDAY (since 07-08 summary):** 3 cards, 4 commits
- #3619 (silas) — landed
- #3431 (kade) — landed (2 commits; native Rust werk lifecycle: worktree add/teardown)
- #2588 (kade) — landed

**TODAY:**
1. **Kade:** `npm ci` at repo root — day 28 and climbing; single command ends test + lint blackout
2. **Silas:** File July card for `bridge-subscriber-watchdog.sh` /tmp state dir (CSC RED, highest risk, no card)
3. **Wren:** Close or re-groom #1704 — 104d, never started; Wren-owned per ops review
4. **Kade:** Investigate +4 type error regression (07-02 origin); still unresolved at day 7

**BLOCKERS (needs Jeff):**
- `npm ci` unrun **day 28** — no owner assigned; needs a person, not a recommendation
- 154 type errors (+4 from 07-02) — unowned since regression started; assign or accept the drift
- #1704 (104d stale WIP) — cannot stay "in_progress" at 100+ days; close, park, or redefine
