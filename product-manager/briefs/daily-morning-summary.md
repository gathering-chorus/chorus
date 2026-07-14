# Daily Morning Summary — 2026-07-14

**HEADLINE:** `npm ci` blackout enters day 33 with no owner — all quality signal still dead; ops added a new YELLOW (CLAUDE.md fragments) while CSC count finally stabilized at 38.

**OPS:** RED (3 REDs carry, 1 new YELLOW)
- RED: Stale WIP — #1759 (Wren, P1) + #1791 (Silas, P1) now **97d** with no commits; close-or-commit overdue
- RED: Domain context — all 5 files ~10d stale (threshold 7d); 20+ cards shipped this week; no refresh filed
- RED: CSC compliance — **38 /tmp refs** in platform scripts (count now stabilized via dedup; 36→67 discrepancy resolved); July migration card still unfiled
- YELLOW (new): CLAUDE.md fragments — 24 shared fragments last touched 2026-07-03, crossed 7d threshold
- YELLOW (carry): LaunchAgent /tmp — 17 plists (down from 21); no card filed
- GREEN: Hooks cargo check clean; git state clean

**QUALITY:** RED — day 33 test blackout, day 34 lint blackout; 154 type errors (day 7, no change)
- 0 tests: 4 suites blocked (clearing, workflow-engine, chorus-sdk, pulse) — ts-jest preset missing
- 0 lint: @eslint/js not found; same root cause
- `npm ci` at repo root is the one-step fix for all of the above

**YESTERDAY (07-13):**
- 1 card shipped: #3639 (Kade)
- #3638 (Kade) re-landed with fix for its own deploy-canonical partition failure (witnessed-skip)
- Routine: Silas ops + quality reviews, Wren morning summary

**TODAY:**
1. **Assign `npm ci`** — day 33; one command unblocks 4 suites + lint; needs an owner this morning
2. **Wren:** Audit 24 CLAUDE.md fragments for drift vs recent card activity (new YELLOW from ops)
3. **Wren:** Close or re-scope #1759 + #1791 (97d, P1) — cannot carry into another week
4. **Silas:** File July CSC migration card — 38 scripts, count stable, scope is now clean
5. **Wren + Silas:** File domain context refresh cards — 10d stale, 3d over threshold

**BLOCKERS (needs Jeff):**
- `npm ci` unrun **day 33** — tests, lint, build all dead; no owner; assign today
- #1759 + #1791 at **97d WIP, P1** — flatlined; explicit close-or-commit call needed
