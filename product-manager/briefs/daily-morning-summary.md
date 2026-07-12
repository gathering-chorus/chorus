# Daily Morning Summary — 2026-07-12

**HEADLINE:** `npm ci` blackout enters day 31 with no owner; CSC /tmp count jumped 36→67 overnight — scope the July card before the number grows again.

**OPS:** RED — 3 REDs carry, no new regressions; count discrepancy is new flag
- RED: CSC compliance — 67 files with /tmp refs (was 36 yesterday); discrepancy may be wider grep scope, needs reconciliation before July card can be filed; `photo-pipeline.py` still highest risk
- RED: Stale WIP — #1759 (Wren/P1) and #1791 (Silas/P1) at 95d; no commits, no close decision
- RED: Domain context — all 5 files 10d stale (last committed 2026-07-01); active shipping into chorus/photos/infra/seeds this week with no refresh filed
- YELLOW: LaunchAgent /tmp — 21 plists logging to /tmp (carry, no card yet)
- GREEN: Hooks cargo check clean; git state clean; CLAUDE.md fragments current (PROTOCOL_VERSION 1.4)

**QUALITY:** RED — day 31 test blackout, day 33 lint blackout; 154 type errors (day 5, unchanged)
- 0 tests: 4 suites blocked (clearing, workflow-engine, chorus-sdk, pulse) — ts-jest preset missing
- 0 lint: @eslint/js not found; same root cause — `npm ci` at repo root fixes both in one step
- Build: 154 TS type errors, no change; +4 regression from 2026-07-02 still unowned
- NO NEW REGRESSIONS introduced today

**YESTERDAY (07-11):**
- Silas: committed ops review (07-11) and quality review (07-12); no card work after morning summary
- All recent card landings (#3632/Kade, #3631/#3629/Silas, #3558/Wren) pre-date yesterday's summary

**TODAY:**
1. **Assign `npm ci`** — day 31; Kade is natural owner; one command ends test + lint blackout
2. **Silas:** Reconcile CSC /tmp count (36→67) — scope must be accurate before filing July card
3. **Wren:** Close-or-commit on #1759 (95d, P1); pull #1791 into same grooming session
4. **Wren + Silas:** Domain context refresh — 10d stale across all 5 domains; ship without refresh = invisible debt

**BLOCKERS (needs Jeff):**
- `npm ci` unrun **day 31** — all tests + lint dead; no owner, no timeline
- #1759 + #1791 at **95d WIP, P1** — both flatlined; needs a close-or-commit call this sprint
