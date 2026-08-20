# Daily Morning Summary — 2026-08-20

**HEADLINE:** The `npm ci` blocker hits day 72 with no owner — now the longest-running unresolved blocker on the board — and type error rate accelerated (+2 today to 233); the two 133-day WIP cards still need a close decision.

---

**OPS:** YELLOW/RED (1 red, 3 yellow, 3 green)
- RED: 2 WIP cards stale 133 days (now +1) — "OWL entity model" + "Restore chorus product boundary." No movement. Decide today.
- YELLOW: Hooks warnings stable at 8 (not shrinking); dead-code cleanup card open but no progress.
- YELLOW: LaunchAgent `/tmp` refs at 17 plists — no migration card filed, no movement.
- YELLOW: Ops-review prompt path stale (`messages/claudemd/` → should be `designing/claudemd/`); minor but self-corrupting.
- GREEN: Git working tree clean; detached HEAD resolved — normal clone today.
- GREEN: Domain context fresh (all 5 domains at 2026-08-15; within threshold; refresh `chorus` soon).
- GREEN: CSC compliance clean.

**QUALITY:** RED (all lanes blocked)
- Tests: 0 run — 4 suites blocked (`ts-jest preset not found`), **day 70**
- Lint: blocked (`@eslint/js`), **day 72**
- Build: 233 type errors (+2 today; was +1/day; six-day total: +16)
- Root cause: `npm ci` unrun at repo root — **72 days. Escalation warranted.**

**YESTERDAY (2026-08-19):** 7 cards shipped
- silas: #3934, #3937, #3742
- wren: #3928 (stop committing generated report — cause removed, tests inverted), #3872
- kade: #3932, #3936

**TODAY: Recommended priorities**
1. **Jeff** — assign owner to `npm ci` (day 72) or formally close the test lane; 72 days is a statement about team priority, not an oversight
2. **Jeff** — decide on the two 133-day WIP cards; close or requeue; they distort the board
3. **Jeff/Wren** — type error rate accelerated today (+2); needs an owner before 233 becomes 300
4. **Silas** — file LaunchAgent `/tmp` migration card; 17 plists confirmed, no card
5. **Silas** — refresh `domain-context-chorus.md`; recent ships (#3742, #3937) make it stale

**BLOCKERS (needs Jeff):**
- `npm ci` — day 72, no owner; tests + lint + coverage all dark; hardest to ignore
- Build errors at 233 and accelerating — needs an owner or explicit acceptance
- Two WIP cards 133 days stale — a board decision, not a fix
