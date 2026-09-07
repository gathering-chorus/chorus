# Daily Morning Summary — 2026-09-07

**HEADLINE:** Hooks compile error and chorus-api are both Day 8 — past escalation threshold; both need a call from Jeff today.

---

**OPS:** 🔴 RED overall
- 🔴 Hooks: `signal_witness.rs:55` — `si_pid`/`si_uid` direct field access (Linux requires method calls). Day 8, past escalation. One-line fix; Silas to land.
- 🔴 chorus-api: Offline Day 8. Board blind — WIP state unverifiable.
- 🟡 CLAUDE.md fragments: 5d stale, threshold 2026-09-08. Wren to refresh today.
- 🟡 Domain context: `domain-context-chorus.md` + `domain-context-infrastructure.md` 5d stale, threshold 2026-09-08.
- 🟡 LaunchAgents: 17 plists logging to `/tmp/` (logs lost on reboot, chronic).
- 🟢 Git: clean. CSC: no new violations.

**QUALITY:** 🔴 RED — no movement, no new failures
- 0 tests run across all 5 suites (ts-jest preset missing, Day 86). Fix: `npm ci` per package.
- Lint blocked Day 88 (`@eslint/js` module not found). Fix: `npm ci` at repo root.
- TS errors: 1481 total across 5 packages, flat. `platform/mcp-server` spike of +222 is 4 days stale — card overdue.

**YESTERDAY:** 5 cards shipped (#4103, #4105, #4106, #4107, #4108). Key: #4114 fixed collection served-name round-trip (list→read 404 loop), #4110 principles read now errors loudly on empty result.

**TODAY:**
1. Jeff: Call on hooks Day 8 and chorus-api Day 8 — both past escalation
2. Silas: Land `si_pid()` / `si_uid()` one-liner in `signal_witness.rs`
3. Wren: Refresh CLAUDE.md fragments + domain-context files before tomorrow's threshold
4. Anyone: `npm ci` at repo root and per package — unblocks Day 86/88 test+lint outage
5. Silas: Open card for `mcp-server` +222 TS error spike (4 days stale)

**BLOCKERS:**
- 🔴 Hooks compile error Day 8 — Jeff needs to explicitly call escalation
- 🔴 chorus-api offline Day 8 — board visibility gone; WIP count unverifiable
- 🔴 Test + lint Day 86/88 — trivial fix (`npm ci`) has not landed; needs owner
