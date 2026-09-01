# Daily Morning Summary — 2026-09-01

**HEADLINE:** chorus-hooks compile failure enters day 2 unpatched and mcp-server tests are newly all-red — quality footprint is growing, not shrinking.

---

**OPS:** 🔴 RED
- 🔴 `chorus-hooks` compile: `signal_witness.rs:55` — `si_pid`/`si_uid` are methods, not fields. **Day 2.** Deploy blocked.
- 🟡 Root CLAUDE.md and domain-context files at 8-day staleness — Wren + Silas refresh both due today
- 🟡 WIP board blind — chorus-api still offline; #3721 disposition unconfirmed
- 🟢 Git working tree clean; LaunchAgent plists: no `/tmp` refs

**QUALITY:** 🔴 RED (deepening)
- Tests: 0 run — `ts-jest` blocked **day 81** (clearing, workflow-engine, chorus-sdk, pulse)
- **NEW:** `platform/mcp-server` — 31 suites, 0 run; babel TS syntax errors (separate root cause from ts-jest)
- Lint: blocked — `@eslint/js` not found, **day 83**
- Build TS errors: clearing 239 · mcp-server 250 · chorus-sdk 28 · workflow-engine 11 · **pulse 952** (first measure — alarming)
- Root fix: `npm ci` at repo root + each sub-package; babel TS preset needed in mcp-server after

**YESTERDAY:** 7 cards shipped (4034–4040)
- #4037 (silas) — nightly agent dual-slot: 03:00 + 13:30, one runner, no second impl
- #4038 (silas) — pulse membrane test: fixed runner-vs-role seam collision (201/201)
- #4039 (silas) — ops-nudge caged: stopped 11+ daily false pages to Jeff from test runs
- #4040 (kade) — ADR-040 KINDS: pipeline + pipeline-step added (PROVISIONAL; 5th generate-vs-write drift)

**TODAY:** Recommended priorities
1. 🔴 Silas: Fix `signal_witness.rs:55` — add `()` to `si_pid`/`si_uid`; rebuild and deploy
2. 🔴 Any role: `npm ci` at repo root + sub-packages — unblocks all test suites (day 81/83)
3. Kade: Fix babel TS plugin in `platform/mcp-server` once `npm ci` lands
4. Silas + Kade: Triage `pulse` 952 TS errors — first reading, severity unknown
5. Wren: Refresh root CLAUDE.md + bump version ledger; Silas: refresh domain-context files

**BLOCKERS — needs Jeff:**
- 🔴 `chorus-hooks` compile broken (day 2) — no deploy until green
- 🔴 `npm ci` blocker (day 81/83) — all test suites blind; needs an owner
- ⚠️ `pulse` 952 TS errors — first measurement; warrants triage before it compounds
- 🟡 chorus-api offline — #3721 disposition still unconfirmed; board snapshot 5 months stale

---
_Sources: ops-review 2026-08-31 · quality-review 2026-09-01 · git log -20 · backlog.md absent · activity.md absent_
_Compiled by wren · 2026-09-01_
