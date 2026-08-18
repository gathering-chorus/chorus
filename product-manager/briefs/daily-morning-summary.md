# Daily Morning Summary — 2026-08-18

**HEADLINE:** Build type errors are accelerating (+14 in three days, now 230) and the npm ci blocker enters day 70 with no owner — both quality lanes need a decision today.

---

**OPS:** YELLOW (ops review 2026-08-17 — 0 red, 5 yellow, 2 green; improvement from yesterday's 4 red)
- YELLOW: Domain context stale — all 5 domains last updated 2026-08-14, now day 4; chorus + infra lag recent shipping
- YELLOW: Hooks dead-code — `probe_role_session` never called (`src/process.rs:76`); warning count unchanged
- YELLOW: LaunchAgent `/tmp` refs — 12+ plists using `/tmp` for log paths; no migration card yet
- YELLOW: Perf-baseline never seeded — scripts exist, no data; first run needed before delta checks work
- GREEN: Git tree clean; CSC compliance check vacuous pass

**QUALITY:** RED (all suites blocked — day 68 / lint day 70)
- Tests: 0 run — 4 suites blocked (`ts-jest preset not found`), day 68
- Lint: blocked (`@eslint/js`), day 70
- Build: 230 type errors (+3 today); three-day trend: 216→217→226→227→230; **+14 in three days, regression accelerating**
- Root cause: `npm ci` unrun at repo root — **70 days, no owner**

**YESTERDAY (2026-08-17):** 4 cards shipped
- wren: #3910 — room signs as bridge with its own registered key; refuses to start without it
- silas: #3386, #3616 — two infra/ops cards (3 commits total)
- kade: #3912 — landed

**TODAY: Recommended priorities**
1. **Jeff / team** — assign an owner to `npm ci` (day 70); or formally close the lane and cancel tests; this is the single highest-leverage unblocked action
2. **Wren** — investigate build type error regression (+14 in three days) before it compounds further
3. **Wren** — content-refresh `domain-context-chorus.md` + `domain-context-infrastructure.md` (day 4 breach, recent shipping warrants update)
4. **Silas** — seed perf-baseline data so disk-delta checks have something to compare
5. **Silas** — file migration card for LaunchAgent `/tmp` log paths

**BLOCKERS (needs Jeff):**
- `npm ci` — **day 70**, no owner; all tests + lint dark; needs an explicit owner or a close decision
- Build errors at 230 and accelerating — if no one is looking at this, it needs a card and an owner today
