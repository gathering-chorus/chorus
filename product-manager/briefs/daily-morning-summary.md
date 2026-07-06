# Daily Morning Summary — 2026-07-06

**HEADLINE:** #3609 landed; Silas can now mint the #3603 graph migration — the product tree Jeff drew is one deploy from matching the live owl-api.

**OPS:** RED (3 REDs, 3 YELLOWs) — Silas review 2026-07-02 (4d stale; no refresh)
- RED: Domain context 14d stale — `domain-context-chorus.md` most urgent, Wren-owned today
- RED: Stale WIPs #1704 (102d), #1759/#1791 (89d) — board unverifiable from remote; planning debt
- RED: CSC /tmp — 58 scripts; `bridge-subscriber.js` runtime inbox highest risk; no card yet
- YELLOW: Hooks dead code 32d; LaunchAgent /tmp (structural); CLAUDE.md fragments 14d lag
- GREEN: Repo clean

**QUALITY:** RED — 0 tests, 0 lint — Kade review 2026-07-02 (4d stale)
- All 4 suites blocked: `ts-jest` preset not found — now **day 25** (clearing, workflow-engine, chorus-sdk, pulse)
- Lint: `@eslint/js` missing — now **day 27**. Same root cause as tests.
- Build: 150 TS type errors — now **day 15**, regression 2026-06-21, unowned
- Fix: `npm ci` at repo root unblocks tests + lint in one shot

**SINCE 2026-07-03:** 6 cards, 7 PRs
- #3609 (wren) — owl-api /batch 4KB body truncation fix; now unblocks #3603 live migration
- #3607 (wren) — chorus.log tail-reads fixed; /api/stream 1650ms → 9–64ms live
- #3608 (wren) — session-registration integrity (P1); role/nudge routing hardened
- #3606 (kade) — 2 PRs; #3604 (kade) — clearing coverage RED → GREEN (first nightly red to zero)
- #3603 (wren) — V1 product-layer retirement shipped; live migration pending Silas

**TODAY:**
1. **Silas:** Mint + apply 53-DEL/352-INS #3603 graph migration — #3609 just cleared the last blocker
2. **Silas or Kade:** `npm ci` at repo root — ends day-25 test blackout in one command
3. **Wren:** Refresh `domain-context-chorus.md` — 14d stale, Silas-flagged today-urgent
4. **Silas:** File July card for `bridge-subscriber.js` CSC /tmp runtime inbox violation
5. **Wren:** CLAUDE.md fragment sweep — #2913 worktree convention + #3581–#3609 lag

**BLOCKERS (needs Jeff):**
- 150 TS type errors 15d unowned — assign a card or it keeps aging
- Stale WIPs 89–102d — close/park them or WIP limit means nothing
- `npm ci` unrun 25d — who owns TS environment health on this repo?
