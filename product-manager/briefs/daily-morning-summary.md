# Daily Morning Summary — 2026-07-02

**HEADLINE:** Quality tooling dark 21+ days and domain context drifting 11 days — both Wren-owned and overdue; Kade eliminated ~37 false nightly reds yesterday, giving clean signal when suites finally run.

---

**OPS:** RED (3 REDs, 3 YELLOWs) — Silas review 2026-07-01, no change
- RED: Domain context freshness — now 11-day drift, 6 chorus cards shipped without refresh; `domain-context-chorus.md` most urgent. Wren-owned.
- RED: Stale WIP — WF-165 (#1704) now 98 days, 0 steps; #1759 [Wren] + #1791 [Silas] now 85 days. Three 80+ day WIPs is active planning debt.
- RED: CSC /tmp compliance — 58 scripts, `bridge-subscriber.js` runtime inbox highest risk. Still no card, no owner.
- YELLOW: Hooks dead code (28d carry), LaunchAgent /tmp refs (structural), CLAUDE.md fragment lag (chorus-prompt.md stale vs. #3581–#3598).
- GREEN: Repo clean.

**QUALITY:** RED — all 4 suites blocked; Kade review 2026-07-02
- 0 tests run: `ts-jest` preset not found — day 21. Root fix: `npm ci` at repo root.
- Build: 150 TypeScript type errors — day 9 (regression from 2026-06-21, unchanged).
- Lint: `@eslint/js` not found — day 23. Same root fix.
- Bright spot: Kade fixed #3571 (nightly parser) + #3589 (lint ratchet) yesterday — ~37 false-reds/night eliminated. Real signal when suites run again.

**YESTERDAY (2026-07-01):** Active shipping day.
- Kade shipped #3602, fixed #3571 (nightly suite parser false-reds) and #3589 (lint-ratchet false-reds). Significant quality infra cleanup.
- Wren shipped #3594.
- No decisions recorded.

**TODAY (recommended priorities):**
1. Wren: Update `domain-context-chorus.md` — RED, 11-day drift, Wren-owned, today.
2. Wren: Refresh `chorus-prompt.md` and CLAUDE.md fragments vs. #3581–#3598.
3. Wren + Silas: Triage #1704, #1759, #1791 — close or park; no card survives 90+ days.
4. Jeff or Kade: Run `npm ci` at repo root — unblocks 4 test suites + lint immediately.
5. Silas: Assign card for `bridge-subscriber.js` /tmp migration (CSC RED, no owner).

**BLOCKERS (needs Jeff):**
- Quality dark Day 21 (RED): `npm ci` unrun — who owns the npm environment on this repo?
- Stale WIPs (RED): WF-165 at 98 days with 0 steps — decision needed to close or re-scope.
- CSC /tmp: `bridge-subscriber.js` runtime inbox is real risk; still no card after ops RED carry.
