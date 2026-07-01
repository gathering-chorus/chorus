# Daily Morning Summary — 2026-07-01

**HEADLINE:** Domain context just escalated to RED after 6 cards shipped without an update, and quality tooling has been dark 23 days — both need same-day action.

---

**OPS:** RED (3 REDs, 3 YELLOWs)
- RED: Domain context freshness (escalated) — 10-day drift, 6 chorus cards (#3581–#3598) shipped since last update; `domain-context-chorus.md` most urgent. Wren-owned.
- RED: Stale WIP — WF-165 (#1704) at 97 days, 0 steps; #1759 [Wren] + #1791 [Silas] at 84 days. Three cards > 80d is planning debt; needs a triage call today.
- RED: CSC /tmp compliance — 58 scripts, `bridge-subscriber.js` runtime inbox highest risk. No card assigned.
- YELLOW: Hooks dead code (27d carry), LaunchAgent /tmp refs (structural), CLAUDE.md fragment lag (chorus-prompt.md stale vs. #3581–#3598).
- GREEN: Repo clean.

**QUALITY:** RED — all packages blocked (Kade review 2026-06-28, day-counts +3)
- 0 tests run: `ts-jest` preset not found — now day 23.
- Build: 150 TypeScript type errors — now day 11; regression from 2026-06-21 unresolved.
- Lint: `@eslint/js` not found — now day 25.
- Root cause: `npm ci` at repo root fixes tests + lint simultaneously. No one has run it.

**YESTERDAY (2026-06-30):** Quiet — no cards shipped. Only Silas ops review committed.
- This week's shipping: Kade #3598 (today), #3596, #3590, #3580; Silas #3593, #3587, #3586, #3584, #3582; Wren #3581.
- No decisions recorded.

**TODAY (recommended priorities):**
1. Wren: Update `domain-context-chorus.md` — RED, escalated, Wren-owned, overdue now.
2. Wren: Refresh `chorus-prompt.md` and stale CLAUDE.md fragments vs. #3581–#3598.
3. Wren + Silas: Triage #1704, #1759, #1791 — close or park all three today.
4. Jeff or Kade: Run `npm ci` at repo root — unblocks 4 test suites and lint immediately.
5. Silas: Assign card for `bridge-subscriber.js` /tmp inbox migration (CSC RED).

**BLOCKERS (needs Jeff):**
- Quality dark Day 23 (RED): `npm ci` unrun — who owns the npm environment on this repo?
- WIP triage (RED): WF-165 97 days with 0 steps completed — close it or re-scope it today.
- CSC /tmp: `bridge-subscriber.js` runtime inbox is a real risk; no card, no owner yet.
