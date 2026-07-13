# Daily Morning Summary — 2026-07-13

**HEADLINE:** 6 cards shipped yesterday (solid velocity); `npm ci` blackout enters day 32 with no owner — tests, lint, and build remain completely dead.

**OPS:** RED (3 REDs carry, no new regressions since 07-12)
- RED: CSC compliance — 67 /tmp refs in platform scripts (3d carry); count discrepancy 36→67 unresolved; no card filed; `photo-pipeline.py` highest risk
- RED: Stale WIP — #1759 (Wren, P1) + #1791 (Silas, P1) now **96d** with no commits or close decision
- RED: Domain context — all 5 files **11d** stale (threshold 7d); no refresh filed despite active shipping across chorus/photos/infra/seeds
- YELLOW: LaunchAgent /tmp — 21 plists logging to /tmp (carry, no card yet)
- GREEN: Hooks cargo check clean; git state clean; CLAUDE.md fragments current (PROTOCOL_VERSION 1.4)

**QUALITY:** RED — day 32 test blackout, day 34 lint blackout; 154 type errors (day 6, no change)
- 0 tests: 4 suites blocked (clearing, workflow-engine, chorus-sdk, pulse) — ts-jest preset missing
- 0 lint: @eslint/js not found; same root cause — `npm ci` at repo root fixes all of this in one step
- 154 TS type errors: held flat for 6 days; no new regressions introduced

**YESTERDAY (07-12):**
- 6 cards shipped: #3638 (deploy-canonical lib partition + witnessed-skip fix — self-fixing card), #3637, #3635, #3421, #3634 (Kade); #3630 (Silas)
- Routine commits: Silas ops review, Kade quality review, Wren morning summary

**TODAY:**
1. **Assign `npm ci`** — day 32; one command unblocks 4 suites + lint; Kade is natural owner
2. **Silas:** Reconcile CSC /tmp count (36→67 discrepancy) — scope must be clean before filing July card
3. **Wren:** Grooming session on #1759 + #1791 (96d, P1) — close or re-scope today
4. **Wren + Silas:** File domain context refresh cards — 11d stale, 4 days over threshold

**BLOCKERS (needs Jeff):**
- `npm ci` unrun **day 32** — all tests + lint dead; no owner, no ETA; needs assignment this morning
- #1759 + #1791 at **96d WIP, P1** — both flatlined; close-or-commit decision needed this sprint
