# Daily Morning Summary — 2026-08-31

**HEADLINE:** Silas shipped 4 cards yesterday fixing the nightly runner — now the hooks binary won't compile, deploy is blocked, and DEC-048 is 28 days stale.

---

**OPS:** 🔴 RED (two hard reds)
- 🔴 `chorus-hooks` binary broken — `cargo check` fails, 2 compile errors at `signal_witness.rs:55` (`si_pid`/`si_uid` are methods, not fields). Binary won't build; deploy blocked until fixed.
- 🔴 chorus-api offline — board scan blocked; #3721 (kade) stale 28 days, Wren's seam card still blocked; DEC-048 decision pending
- 🟡 Root CLAUDE.md 7 days old — refresh due today (Wren action)
- 🟡 Domain-context files 7 days old — Silas refresh due (10+ cards shipped to chorus/infra domains since last update)
- 🟡 168 `/tmp/` refs in scripts; 35 LaunchAgent plists unmigraded — CSC cards open, no regression

**QUALITY:** 🔴 RED across the board (no change)
- Tests: 0 run — `ts-jest` preset not found, **day 80** (4 suites: clearing, workflow-engine, chorus-sdk, pulse)
- Lint: blocked — `@eslint/js` not found, **day 82** (root `node_modules` absent)
- Build: **239 TS errors** — stable, no new regression
- Root fix (still open): `npm ci` at repo root + each sub-package

**YESTERDAY:** 5 cards shipped
- #4032 (silas) — launchd runner CHORUS_HOME fix (190/190 nightly units were refused; now wired correctly)
- #4033 (silas) — per-unit store index collision fix (only 3 of 190 units stored; now all land)
- #4029, #4030 (silas) — merged
- #4031 (wren) — merged

**TODAY:** Recommended priorities
1. 🔴 Silas: Fix `signal_witness.rs:55` — `si_pid()` / `si_uid()` (add parens); rebuild and deploy
2. 🔴 Jeff: Decision on #3721 (DEC-048) — 28 days stale, Wren's seam card blocked
3. Silas: Refresh `domain-context-chorus.md` + `domain-context-infrastructure.md`
4. Wren: Refresh root CLAUDE.md (at 7-day threshold)
5. Any role: `npm ci` at repo root to unblock quality infra (day 80/82 chronic)

**BLOCKERS (needs Jeff):**
- 🔴 **#3721 (kade) — 28 days stale, DEC-048 decision still pending; Wren's seam card remains blocked**
- 🔴 **`chorus-hooks` compile broken — deploy blocked until Silas lands fix**
- 🔴 **chorus-api offline — board blind; full stale WIP count unknown**

---
_Sources: ops-review 2026-08-30 · quality-review 2026-08-31 · git log -20 · backlog.md absent · activity.md absent_
_Compiled by wren · 2026-08-31_
