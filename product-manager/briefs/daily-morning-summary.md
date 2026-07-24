# Daily Morning Summary — 2026-07-24

**HEADLINE:** Quality toolchain blocked day 45 + new build regression from yesterday's merge — `npm ci` and a one-liner type fix are the unlock.

**OPS:** YELLOW/RED (Silas, 2026-07-23)
- GREEN: Hooks build clean; all 7 role dirs zero uncommitted changes
- YELLOW: 17+2 LaunchAgent plists still reference `/tmp/` (carry); CLAUDE.md fragments 11d stale, 4d over threshold (Wren overdue)
- RED: CSC compliance +1 (now 37 sh files with `/tmp/` refs — new file needs identification); domain-context 9d stale after 7 cards shipped (`domain-context-chorus.md` + `domain-context-infrastructure.md` critical); #1759/#1791 at 107d no commits

**QUALITY:** RED (Kade, 2026-07-24)
- All 4 suites blocked: `ts-jest` preset not found — day 43; lint blocked (`@eslint/js`) — day 45
- **NEW REGRESSION:** Build 154 → 157 type errors (+3) from kade:#3667 (merged Jul 23) — `_res`/`next` implicit `any` in `server.ts:132`; fix is explicit Express types
- `npm ci` at repo root unblocks everything; 45 days unresolved

**YESTERDAY (07-23):** 11 cards merged
- **#3617 (silas):** Alert truthfulness — cron schedules now real (cron-due.py), Loki checks scoped, latency spec honest; surfaces warm=556ms vs 400ms spec (real drift, not masked)
- **#3667 (kade):** Clearing remote tabs — token-authed GET for read pair + domain-detail proxy (introduced +3 TS regression)
- **#3668 (wren):** Clearing→VS Code delivery via tmux (osascript load-buffer/paste-buffer)
- **#3676 (wren):** Clearing service-design page refreshed; #3664/#3665/#3670/#3672/#3673/#3675/#3592 also closed

**TODAY:**
1. **Kade:** Add explicit Express types to `server.ts:132` — stops regression before it grows
2. **Jeff → `npm ci`:** Day 45; all 4 suites dark; assign owner or close permanently
3. **Silas:** Identify new `/tmp/`-referencing sh file (CSC count 36→37)
4. **Silas/Kade:** Refresh `domain-context-chorus.md` + `domain-context-infrastructure.md` (9d stale, multiple cards shipped to both)
5. **Wren:** CLAUDE.md claudemd refresh — 11d stale, escalation threshold passed

**BLOCKERS (needs Jeff):**
- **`npm ci` day 45** — quality fully dark, no owner; if there's an environmental block, surface it now
- **#1759/#1791 107d** — must close, archive, or reassign; #3607 escalation still open
- **Latency drift:** `chorus-hook-shim` warm=556ms vs 400ms spec (#3617 surfaced honestly); not a fire but needs a decision
