# Morning Summary — 2026-09-08

**HEADLINE:** Hooks compile error and chorus-api are both Day 9 with escalation overdue — two critical infrastructure failures need Jeff's call today.

---

**OPS** — 🔴 RED (from Silas, 2026-09-07)
- 🔴 Hooks: `signal_witness.rs:55` compile error, Day 9. Escalation was due Day 8. One-line fix (`si_pid()`/`si_uid()` method calls). Silas can land it; Jeff needs to make the call.
- 🔴 chorus-api: OFFLINE Day 9. Board completely blind — WIP card state unverifiable.
- 🟡 CLAUDE.md fragments: 5 days stale, **2-day runway before RED** — Wren must refresh today.
- 🟡 Domain context: `designing/domain-context/*.md` 5 days stale, **2-day runway** — update today.
- 🟢 Git: clean. CSC: no new violations.

**QUALITY** — 🔴 RED (from Kade, 2026-09-08)
- 0 tests run. All 4 ts-jest suites blocked, Day 87. Lint blocked Day 89. Fix: `npm ci` per package.
- TS errors flat: 1,481 across 5 packages. mcp-server +222 spike now 5 days stale — card needed.
- ⚠ +1 new failure: mcp-server grew 31 → 32 failing suites. A new test file added to a broken suite.
- Coverage: N/A (all suites blocked).

**YESTERDAY** — 6 cards shipped
- #4114 (wren): Fixed /cards and /principles 404 loop — list now serves the name read accepts; principles were minting wrong prefix (28 rows broken). Major correctness fix.
- #4116 (wren): Card row now carries its owner — land can write only the rows it merged.
- #4113 (wren): Wren card landed.
- #4111 (kade): Kade card landed.
- #4110 (silas): Principles read now surfaces "lost all rows" error instead of silent empty answer.
- #2436 (silas): Silas card landed.

**TODAY** — recommended priorities
1. Jeff: Call Silas on hooks fix — Day 9 is past threshold. Two-minute code change, needs authorization.
2. Jeff: chorus-api recovery — 9 days blind is unacceptable for board health.
3. Wren: Refresh CLAUDE.md fragments and domain-context-chorus.md — deadline is today or tomorrow goes RED.
4. Any role: `npm ci` across packages to unblock tests/lint (Day 87/89 is embarrassing — this is a one-command fix).
5. Silas: Open card for mcp-server +222 TS spike (5 days old, no card).

**BLOCKERS** — needs Jeff
- 🔴 Hooks compile error Day 9: escalation overdue. Silas has the fix; Jeff must authorize the land call.
- 🔴 chorus-api Day 9 offline: board blind. Restoration needs infrastructure access.
- 🔴 Test/lint Day 87/89: `npm ci` fix has been obvious for 3 months. Needs a dedicated burn-down session.
