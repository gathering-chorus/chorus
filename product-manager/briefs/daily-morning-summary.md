# Daily Morning Summary — 2026-08-19

**HEADLINE:** The `npm ci` blocker enters day 71 with no owner — tests, lint, and build all dark — and two WIP cards have been stale 132 days; both need a decision today.

---

**OPS:** YELLOW/RED (1 red, 4 yellow, 2 green)
- RED: 2 WIP cards stale 132 days (last touched 2026-04-07) — "OWL entity model" + "Restore chorus product boundary." Close or requeue.
- YELLOW: Hooks warnings up to 8 (was 7); dead-code accumulation continues — `owes_response_block`, `build_reply_event`, etc.
- YELLOW: LaunchAgent `/tmp` refs now confirmed at 17 plists (up from 12+); no migration card filed.
- YELLOW: Clone landed in detached HEAD (`5515d27`); investigate scheduled-session checkout.
- GREEN: Domain context fresh (all 5 domains updated 2026-08-15, within 7-day threshold). CSC clean.

**QUALITY:** RED (all lanes blocked)
- Tests: 0 run — 4 suites blocked (`ts-jest preset not found`), **day 69**
- Lint: blocked (`@eslint/js`), **day 71**
- Build: 231 type errors (+1 today; +14 over five days; trend accelerating)
- Root cause: `npm ci` unrun at repo root — **71 days. Escalation warranted.**

**YESTERDAY (2026-08-18):** 7 cards shipped
- silas: #3915, #3917, #3918, #3926, #3927
- wren: #3911
- kade: #3913

**TODAY: Recommended priorities**
1. **Jeff** — assign owner to `npm ci` (day 71) or formally close the test lane; highest-leverage unblocked action
2. **Jeff** — decide on the two 132-day WIP cards; they are dead weight on the board
3. **Wren** — investigate build type error regression (+14 in 5 days) before 231 becomes 250
4. **Silas** — file LaunchAgent `/tmp` migration card; 17 plists confirmed, no card exists
5. **Wren** — refresh `domain-context-chorus.md` at next session given recent ships

**BLOCKERS (needs Jeff):**
- `npm ci` — day 71, no owner; all tests + lint dark; needs owner or explicit close decision
- Build errors at 231 and accelerating — needs a card and an owner if no one is on it
- Two WIP cards 132 days stale — board hygiene decision needed
