# Daily Morning Summary — 2026-07-26

**HEADLINE:** TS build is on a two-day regression streak (157→174→181, +24 errors) and Buzz Leg A shipped clean yesterday — fix the type rot before it hits 200.

**OPS:** YELLOW/RED (Silas, 2026-07-25)
- GREEN: Git clean; 9 cards merged Jul 25 — active velocity
- YELLOW: Dead-code warnings carry (4 lines, both binaries); 19 plists still referencing `/tmp/`; CLAUDE.md fragments **13d stale, 2nd missed deadline — Wren escalates to Jeff today**
- RED: CSC compliance static (37 sh files, no progress); WIP cards #1759/#1791 at 109d without commits; all 5 domain context files 97–122d stale despite 9 more cards landing yesterday

**QUALITY:** RED (Kade, 2026-07-26)
- All 4 suites blocked: `ts-jest` preset not found — day 45; lint blocked (`@eslint/js`) — day 47
- **Build: 181 type errors (+7 today, second consecutive regression day — two-day: 157→174→181)**
- `npm ci` at repo root unblocks everything; **47 days unresolved**

**YESTERDAY (07-25):** 9 cards shipped
- **#3696 (wren):** Clearing speaks Buzz (Leg A) — signed kind:9 mirror, 21 unit tests, full suite 426 green, live relay verified
- **#3674 (wren):** DECISION — steal-the-pattern (self-host relay, root in our identity; NOT Block's hosted app)
- **#3686 (wren):** Role-level hard priorities, set verb (9 tests, /sup walk green)
- **#3684 (silas):** build-signed.sh non-macOS guard (quality.yml RC1)
- **#3695, #3356 (silas); #3692, #3392, #3288, #3683 (kade/wren)** — also closed

**TODAY:**
1. **Wren + Silas:** Audit commits since Jul 25 for today's +7 TS errors — stop before 200
2. **Jeff → `npm ci`:** Day 47; all 4 suites dark; needs a decision or an owner
3. **Wren:** CLAUDE.md claudemd refresh — 2nd missed deadline; escalate to Jeff now
4. **Silas:** Refresh `domain-context-chorus.md` + `domain-context-infrastructure.md` (97–122d stale)
5. **Silas/Wren:** Close or archive #1759/#1791 (109d without commits)

**BLOCKERS (needs Jeff):**
- **`npm ci` day 47** — quality fully dark; 4 suites blind; surface the block or make the call
- **CLAUDE.md refresh** — 13d stale, 2nd missed deadline; Wren escalating today
- **#1759/#1791 109d** — must close, archive, or reassign
