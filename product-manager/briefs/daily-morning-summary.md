# Morning Summary — 2026-08-24

**HEADLINE:** npm ci enters day 76 unresolved; 7 cards shipped yesterday after the brief — strong velocity, broken test layer.

---

**OPS:** YELLOW (Silas, 2026-08-23)
- 3 yellows, 0 reds, 1 unknown — no change in category count
- Improvement: chorus-hooks warnings down to 7 (was 8; shim warning pruned post-#2804). Still need to clear remaining 7.
- Persistent: 13+ plist files log to /tmp; 14+ scripts use /tmp — werk-init.sh and bedroom-heartbeat flagged for CSC review
- Unknown: disk delta — perf-baseline.sh needs a live run; flag if >2% growth

**QUALITY:** RED (Kade, 2026-08-23)
- 0 tests run — ts-jest preset not found across all 4 suites (clearing, workflow-engine, chorus-sdk, pulse) — **day 74**
- Lint blocked (@eslint/js not found) — **day 76**
- Build: YELLOW — 234 type errors, holding flat after first decrease Monday. Trend is stalled.
- Root cause: `npm ci` at repo root. One command. 76 days.

**YESTERDAY:** 7 cards shipped after the morning brief — silas: #3993 (binary signature gate in chorus-bin-install, exit 10 on unsigned, 4/4 bats), #3991, #3968 (alert-delivery-test bridge probe fix, live 6/6), #3682, #3381; kade: #3989 (cards CLI single-pid spawn, orphan-reaper narrowed, 5/5 bats), #3753. Strong ops and infra push.

**TODAY:**
1. **npm ci** — day 76. No tests, no lint, no coverage. Run it or formally decide to skip (but document it).
2. Prune 7 remaining chorus-hooks dead symbols (#2804 cleanup) — small lift, Silas/Wren
3. CSC audit: confirm werk-init.sh session cache + bedroom-heartbeat use $TMPDIR, not /tmp
4. Silas: run perf-baseline.sh and check disk delta — repo at 740MB, trend unknown
5. Monitor build type-error count — 234 for two days; want to see it move again

**BLOCKERS (needs Jeff):**
- `npm ci` — **day 76, no change.** All tests, lint, coverage blocked. Yesterday's call wasn't made; today's the day.
