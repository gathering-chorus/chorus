# Morning Summary — 2026-08-22

**HEADLINE:** Quality is still blind at day 74 and `npm ci` is 74 days overdue — escalation is overdue.

---

**OPS:** YELLOW (improved from RED)
- YELLOW: 7 cargo warnings in chorus-hooks, stable. Cleanup card open.
- YELLOW: LaunchAgent plists still log to `/tmp/` — no migration to `~/.chorus/logs/`.
- YELLOW: Domain context freshened to 4 days (was 14) — Silas refreshed chorus/infrastructure. Monitor; refresh threshold is 7 days.
- RED: 2 WIP cards stale **135 days** — "Framework service design — OWL entity model" and "Restore chorus product boundary." One more day, no action.

**QUALITY:** RED
- All 4 TS test suites blocked: `ts-jest preset not found` — **day 72**.
- Lint blocked: `@eslint/js` not found — **day 74**. Root cause: `npm ci` never run at repo root.
- Build: 235 type errors — **first plateau** in 7 days (trend was +18 over prior week). Worth watching.
- 0 tests have run in 72 days. Coverage data is stale from June.

**YESTERDAY:** Active day across all three roles — 15+ cards shipped.
- Silas: #3967, #3969, #3966, #3965, #3963, #3960, #3958, #3729, #3949
- Kade: #3972, #3970, #3601, #3964, #3920, #3956
- Wren: #3959, #3561 (×2)

**TODAY:**
1. **Anyone:** `npm ci` at repo root — this is now 74 days and an explicit escalation. Unblocks 4 suites and lint in one command.
2. **Wren:** Close or requeue the two 135-day WIP cards — they are off the radar.
3. **Silas:** Watch domain context — 3 remaining files will hit 7-day threshold by 2026-08-24.
4. **All:** Monitor type error count — first plateau today; if it resumes climbing, investigate recent Kade/Silas lands.

**BLOCKERS (needs Jeff):**
- `npm ci`: 74 days. Is skipping it a deliberate policy (remote-only CI)? If so, document it so quality review stops flagging it. If not, this needs to run today.
- 135-day WIP cards: close them or assign them to a role with a due date — they cannot stay on the board.
