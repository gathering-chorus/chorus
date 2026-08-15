# Daily Morning Summary — 2026-08-15

**HEADLINE:** Domain-context breach hits day 3 and CLAUDE.md refresh is due tomorrow — Wren must close both today.

---

**OPS:** RED/YELLOW
- RED (d3): Domain-context files — chorus + infrastructure months stale, 10+ cards shipped since April with no update; breach escalates daily; Wren owns NOW
- RED: CSC compliance — 152 `/tmp/` refs in `platform/scripts/`, unchanged from yesterday; no migration card, no movement
- YELLOW: Hooks dead-code — 4 warnings (up from 3 stable, set expanding); day 12; address before day 14
- YELLOW: CLAUDE.md fragments v1.6.0 — **refresh due Aug 16 (tomorrow)**; 1 day left on Wren's commitment
- YELLOW: LaunchAgent `/tmp` refs — 20+ plists, >14d stall; migration card must exist and be assigned
- YELLOW: Zombie card #1962 — 124d in "building"; Wren to close from state.json
- GREEN: Git state clean (0 uncommitted)

**QUALITY:** RED (all suites blocked, day 65)
- Tests: 0 run — 4 suites blocked by `ts-jest preset not found`, day 65
- Lint: blocked by `@eslint/js`, day 67
- Build: **217 type errors (+1 NEW)** — regression continues; `transcript.ts:171,173` (nullable string), `word-cap.ts:115` (`@types/node`)
- Root cause: `npm ci` unrun, **67 days, no owner**

**YESTERDAY (2026-08-14):** Active shipping day
- #3869 (wren) — role cards stop lying; state derived from observed activity, failed board poll no longer blanks WIP
- #3868 (wren) — code comment rendering fix; scoped leak check to page chrome
- #3866 (kade) — landed
- Also closed: #3865, #3862, #3863, #3845, #3861

**TODAY (2026-08-15, already landed):** 6 cards this morning
- #3881 (silas) — ADR-058 registry deploys; governance-checks-3846.ttl joins MODEL_SET
- #3876 (wren) — word-cap edges move to role's home graph; /effective now sees them
- Also: #3878 (wren), #3879 (silas), #3880 (silas), #3846 (silas)

**TODAY: Recommended priorities**
1. **Wren** — update `domain-context-chorus.md` + `domain-context-infrastructure.md` (d3 breach; also closes CLAUDE.md refresh if done together)
2. **Wren** — CLAUDE.md fragment bump to v1.6.1+ before Aug 16 EOD
3. **Wren** — close zombie card #1962 from state.json (124d)
4. **Silas** — assign CSC migration card (152 refs, no movement); assign LaunchAgent /tmp migration card (>14d stall)
5. **All** — identify npm ci owner or formally close the test lane (67d unowned)

**BLOCKERS (needs Jeff):**
- `npm ci` — **day 67**, no owner; all 4 suites + lint dark; decision needed: owner or deliberate lane closure
- Domain-context breach — d3, protocol violation; Wren accountable; closes tomorrow if not done today
- CSC 152 refs — floor unchanged, no migration plan visible; Silas to escalate or assign
