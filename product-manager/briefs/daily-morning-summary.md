# Daily Morning Summary — 2026-07-25

**HEADLINE:** TS build regression hit +17 in one day (now 174 errors) — wren:#3679 and silas:#3669 both need type fixes before errors compound further.

**OPS:** YELLOW/RED (Silas, 2026-07-24)
- GREEN: Git clean across all role dirs
- YELLOW: 2 new dead-code warnings in cargo check (regression vs yesterday's clean pass); 17+2+14 LaunchAgent plists still referencing `/tmp/`; CLAUDE.md fragments 12d stale — Wren missed EOD Jul 23 deadline
- RED: CSC compliance static at 37 sh files with `/tmp/` refs — no progress, no regression; domain-context 9d stale after 5 more cards landed yesterday; #1759/#1791 now 108d without commits

**QUALITY:** RED (Kade, 2026-07-25)
- All 4 suites blocked: `ts-jest` preset not found — day 44; lint blocked (`@eslint/js`) — day 46
- **NEW REGRESSION:** Build 157 → 174 type errors (+17, largest single-day jump in 2 weeks) — implicit `any` params from wren:#3679 (account/change-password routes) and silas:#3669 (CSS-OIDC, WS session auth); fix is `import { Request, Response, NextFunction } from 'express'` in `server.ts`, `account.ts`, `solid-auth.ts`, `solid-oidc.ts`, `connection-auth.ts`
- `npm ci` at repo root unblocks everything; 46 days unresolved

**YESTERDAY (07-24):** 10 cards merged — strong velocity
- **#3679 (wren):** Account page + change-password route over CSS API; 8 security tests green — but introduced +17 TS errors
- **#3681 (silas):** DAL uniqueness-within-scope enforcement (uniqueWithin/uniqueGlobal)
- **#3669 (silas):** Clearing CSS-OIDC edge: human login + WS session auth, static token retiring
- **#3678 (kade):** Checking on a pipeline never starts new work
- **#3654 (wren), #3623 (kade), #2478 (kade), #3680 (kade), #2513 (kade), #3480 (kade)** also closed

**TODAY:**
1. **Wren + Silas:** Add explicit Express types to new route files — stop TS regression before it hits 200
2. **Jeff → `npm ci`:** Day 46; 4 suites fully dark; this needs an owner or a permanent decision
3. **Wren:** CLAUDE.md claudemd refresh — 12d stale, deadline missed, escalating now
4. **Silas/Kade:** Refresh `domain-context-chorus.md` + `domain-context-infrastructure.md` (9d stale)
5. **Silas:** Suppress/remove 2 dead-code warnings; add canonical-14 plists to `/tmp/` tracking scope

**BLOCKERS (needs Jeff):**
- **`npm ci` day 46** — quality fully dark; if there's an environmental block, it needs to surface now
- **#1759/#1791 108d** — must close, archive, or reassign; #3607 escalation still open
- **CLAUDE.md refresh** — 12d stale, Wren missed deadline; if not landed today, needs Jeff's call on scope
