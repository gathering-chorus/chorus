# Daily Morning Summary — 2026-07-10

**HEADLINE:** Domain context was falsely GREEN yesterday — all 5 files are 81–106d stale; `npm ci` still unrun at day 29.

**OPS:** RED — 3 REDs carry, 2 GREENs resolved
- RED: Domain context — all 5 files stale by content date (chorus 81d, infra 106d, music/photos/seeds 99–105d); yesterday's GREEN was a false read off clone mtime; refresh sweep due this sprint
- RED: CSC compliance — 7 hardcoded /tmp paths in role scripts (kade ×5, wren ×2); `photo-pipeline.py` highest risk, no July card filed yet
- RED: Stale WIP — #1759 (Wren/P1, 93d) and #1791 (Silas/P1, 93d); both last touched 2026-04-07
- GREEN (resolved): Hooks — cargo check 0 warnings, dead-code carry from yesterday cleared
- YELLOW: LaunchAgent /tmp — 17 plists, structural carry

**QUALITY:** RED — day 29 test blackout, day 31 lint blackout; 154 type errors unchanged (day 3)
- 0 tests: all 4 suites (clearing, workflow-engine, chorus-sdk, pulse) blocked — ts-jest preset missing
- 0 lint: @eslint/js missing — same root cause; `npm ci` at repo root fixes both in one step
- Build: 154 TS type errors, no change; +4 regression from 2026-07-02 still unowned
- NO NEW REGRESSIONS introduced today

**YESTERDAY (since 07-09 summary):** 4 cards, 6 commits
- #3632 (kade) — 3 commits, landed
- #3631 (silas) — landed
- #3558 (wren) — landed
- #3629 (silas) — spine events: registered 3 #3431 teardown emits (conformance, additive-only)

**TODAY:**
1. **Kade:** `npm ci` at repo root — day 29; one command ends test + lint blackout
2. **Silas/Wren:** Domain context refresh sweep — Silas owns chorus/infra, Wren owns music/photos/seeds; this sprint
3. **Silas:** File July card for `photo-pipeline.py` /tmp state (CSC RED, highest risk, no card)
4. **Wren:** Close or re-groom #1759 (93d WIP, Wren-owned P1)

**BLOCKERS (needs Jeff):**
- `npm ci` unrun **day 29** — no owner; needs a person assigned, not a recommendation
- Domain context all stale (false GREEN yesterday) — 5 files, 81–106d; no refresh sprint started
- #1759 + #1791 at 93d WIP — both P1, both dead; needs a close-or-commit decision
