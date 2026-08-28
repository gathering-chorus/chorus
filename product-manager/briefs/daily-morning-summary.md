# Morning Summary — 2026-08-28

**HEADLINE:** Build fire out overnight (946→238 TS errors, -708 recovered); domain-context-chorus hits the 7-day staleness threshold today, and chorus-api is still down.

---

**OPS:** YELLOW/RED (1 red, 4 yellows)
Top concern: 16+ LaunchAgent plist files use hardcoded `/tmp/` for log paths — CSC non-compliant. No card exists yet. File one today.
- 🔴 RED: LaunchAgent `/tmp/` refs in `proving/config/launchagents/` + `chorus-hooks/` — CSC violation, needs card + migration to `$TMPDIR` or `$CHORUS_HOME/logs/`.
- 🟡 chorus-api OFFLINE (ConnectionRefused) — board state unavailable; confirm it's up before stand-up.
- 🟡 domain-context-chorus: 6 days old as of today, 1,287 file touches in 7 days — **at threshold, refresh today**.
- 🟡 Hooks: 7 `cargo check` warnings in chorus-hooks (`probe_role_session` + others) — accumulated tech debt.
- 🟡 Perf baseline data: scripts exist but no captured data visible remotely; confirm host is capturing.

**QUALITY:** RED
- 4 test suites (clearing, workflow-engine, chorus-sdk, pulse): BLOCKED — `ts-jest` preset not found. **Day 77.** Root fix: `npm ci` in each sub-package.
- Lint: BLOCKED — `@eslint/js` not found (root `node_modules` missing). **Day 79.**
- Build: **238 TS errors** — recovered from yesterday's 946 (-708). @types/jest spike resolved; back to prior baseline.
- Tests run: **0**. Coverage: N/A.

**YESTERDAY (2026-08-27):** 5 cards shipped — #3860 (wren ×2 commits), #3992 (silas), #4015 (kade), #4020 (silas), #4021 (silas). Strong throughput. Build spike resolved itself overnight (no explicit fix needed).

**TODAY (recommended priorities):**
1. **[Wren]** Refresh `domain-context-chorus.md` — at the 7-day cliff, high-activity domain.
2. **[Silas]** File card for LaunchAgent `/tmp/` → `$TMPDIR` migration; 16+ files, CSC blocking.
3. **[Silas]** Bring chorus-api back online — board is blind without it.
4. **[Jeff decision]** Test suite + lint blocker at day 77/79: who owns `npm ci` across sub-packages? Escalation is overdue.

**BLOCKERS (needs Jeff):**
- **chorus-api down** — board state unavailable; stand-up is flying blind on WIP.
- **npm ci blocker, day 77** — 4 test suites and lint blocked for 11 weeks. No owner, no timeline. Needs a decision.
