# Daily Morning Summary — 2026-07-03

**HEADLINE:** Quality still dark day 21 — `npm ci` at repo root unblocks all 4 suites immediately; ops unchanged, Wren owns two overdue refreshes today.

---

**OPS:** RED (3 REDs, 3 YELLOWs) — Silas review 2026-07-02, no change from yesterday
- RED: Domain context — all 5 files 11d stale; `domain-context-chorus.md` most urgent (6 chorus cards shipped since last update). Wren-owned today.
- RED: Stale WIP — #1704 (99d), #1759/#1791 (86d); board unverifiable from remote; planning debt compounding daily.
- RED: CSC /tmp — 58 scripts, `bridge-subscriber.js` runtime inbox highest risk; still no card, no owner.
- YELLOW: Hooks dead code (28d), LaunchAgent /tmp refs (structural), CLAUDE.md fragments 11d stale (lag vs. #3581–#3602).
- GREEN: Repo clean.

**QUALITY:** RED — all 4 suites blocked, 0 tests run; Kade review 2026-07-02
- Tests: `ts-jest` preset not found — day 21 across clearing, workflow-engine, chorus-sdk, pulse.
- Lint: `@eslint/js` missing — day 23. Same root cause.
- Build: 150 TypeScript type errors — day 12, regression from 2026-06-21, unowned.
- Fix: `npm ci` at repo root resolves tests + lint in one shot.

**YESTERDAY (2026-07-02):** 3 cards shipped (PRs #721–723).
- #3597 (kade) — nightly-suites determinism (3 behavioral fixes to `nightly-suites.sh`).
- #3536 (silas) — PR #722.
- #3600 (kade) — PR #721.
- Silas wrote daily ops + quality reviews.

**TODAY (recommended priorities):**
1. Kade or Silas: `npm ci` at repo root — kills the 21-day test blackout in one command.
2. Wren: Refresh `domain-context-chorus.md` — Silas flagged today-urgent (RED, 11d).
3. Wren: Sweep CLAUDE.md fragments for #2913 worktree convention and #3581–#3602 lag.
4. Silas: File July card for `bridge-subscriber.sh` /tmp CSC violation.
5. Team: Close or park #1704, #1759, #1791 — 80+ day WIPs erode planning credibility.

**BLOCKERS (needs Jeff):**
- 150 type errors since 2026-06-21 (12d) — no owner, no card; assign or it keeps aging.
- Stale WIPs 80–99 days: call them done/parked or WIP limit is fiction.
- `npm ci` unrun for 21 days: who owns the TS environment health on this repo?
