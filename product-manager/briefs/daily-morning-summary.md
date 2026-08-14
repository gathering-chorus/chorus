# Daily Morning Summary — 2026-08-14

**HEADLINE:** Build regression grew to 216 type errors (+3 today) and the domain-context breach enters day 2 — both need same-day action.

---

**OPS:** YELLOW/RED
- RED (d2): Domain-context files — breach flagged yesterday (Aug 13), still unresolved; chorus + infrastructure domains 4d stale with 11 cards shipped since. Wren owns NOW.
- YELLOW: CSC compliance — 152 refs (-4 from 156); some progress, confirm what landed, 152 is the new floor
- YELLOW: Hooks dead-code — 3 paths, day 11 (stable)
- YELLOW: LaunchAgent `/tmp` refs — 20+ plists, >13d stall, confirm card exists
- YELLOW: Zombie card #1962 — 123d in "building"; Wren to close from state.json
- GREEN: Git state clean; CLAUDE.md fragments at v1.6.0

**QUALITY:** RED (all suites blocked, day 64)
- Tests: 0 run — 4 suites blocked by `ts-jest preset not found`, day 64
- Lint: blocked by `@eslint/js`, day 66
- Build: **216 type errors (+3 NEW regression)** — `transcript.ts:171,173` (nullable string narrowing), `word-cap.ts:115` (`@types/node` missing)
- Root cause: `npm ci` unrun, **66 days, no owner**

**YESTERDAY:** High-velocity — 11 cards shipped (mostly Wren)
- #3865 (wren) — Jeff's browser key now generates; import map + extensionless vendor resolution fixed
- #3862 (wren) — task-notification XML no longer renders as Jeff's own messages (regression from same card)
- #3850 (silas) — load-attribution classifier + negative proof; 32 real werk-context defects
- Also landed: #3863, #3861, #3858, #3857, #3854, #3853, #3852, #3851, #3844, #3843

**TODAY:** Recommended priorities
1. **Wren** — `domain-context-chorus.md` + `domain-context-infrastructure.md` update (d2 breach, overdue)
2. **Kade** — fix `transcript.ts:171,173` (nullable → string) and `word-cap.ts:115` (`@types/node`) — new regression, fix before it compounds
3. **Silas** — confirm what cleared 4 CSC refs; plan next batch; assign LaunchAgent `/tmp` migration card
4. **All** — identify owner for `npm ci` fix or formally close the test lane (66d unowned)
5. **Wren** — close zombie card #1962 from state.json (123d, cleanup)

**BLOCKERS (needs Jeff):**
- `npm ci` — **day 66**, no owner; all 4 test suites + lint dead; needs an owner or a deliberate decision to close
- Domain-context breach — protocol violation, now day 2; Wren accountable
