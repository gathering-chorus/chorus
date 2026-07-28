# Daily Morning Summary — 2026-07-28

**HEADLINE:** Strong 4-card velocity yesterday but `npm ci` enters day 49 — quality remains fully dark and domain context hits 100d stale.

**OPS:** YELLOW/RED (Silas, 2026-07-27)
- GREEN: Git clean; 4 more cards merged since yesterday's summary (#3701/#3690/#3702/#3687) — velocity holds
- YELLOW: Dead-code warnings carry (4 lines); 19 plists with `/tmp/` refs; CLAUDE.md **role fragments still unrefreshed** (4-card delta since Jul 23)
- RED: CSC compliance static (37 sh files, no progress); WIP cards #1759/#1791 at **111d** — must close or archive; all 5 domain context files **≥4d stale**, chorus context ~**100d** with 6+ cards shipped into it

**QUALITY:** RED (Kade, 2026-07-28)
- All 4 suites blocked: `ts-jest` preset not found — **day 47**; lint blocked (`@eslint/js`) — **day 49**
- Build: **181 type errors (+0)** — stable for second consecutive day; regression trend paused
- Primary fix: `npm ci` at repo root — **49 days unresolved**

**YESTERDAY (07-27):** 4 cards shipped
- **#3701 (wren):** Card closed
- **#3690 (silas):** Card closed
- **#3702 (wren):** Card closed
- **#3687 (silas):** Card closed

**TODAY:**
1. **Jeff → `npm ci`:** Day 49; 4 suites blind since early June — needs owner or decision
2. **Silas:** `domain-context-chorus.md` critical at ~100d; update before more Chorus cards land
3. **Wren + Kade:** Role fragment refresh in CLAUDE.md — 4-card delta since last touch Jul 23
4. **Wren:** Close or archive #1759/#1791 — 111d stale, no path forward

**BLOCKERS (needs Jeff):**
- **`npm ci` day 49** — quality fully dark; 4 suites blind, coverage unknown since early June
- **#1759/#1791 at 111d** — board noise; must resolve today
