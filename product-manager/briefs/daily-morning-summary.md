# Daily Morning Summary — 2026-08-04

**HEADLINE:** New mcp-server ESM failure joins 54-day test blockage and two day-4 REDs — `npm ci` is now the highest-priority unowned item on the board.

**OPS:** RED — 3 reds, 3 yellows (Silas, 2026-08-03)
- GREEN: Git clean across all tracked directories
- YELLOW: Hooks regressed 1→2 warnings (`registration_json` back after merges, `owes_response_block` carries); LaunchAgent /tmp 33/17 no movement; Dependabot #449/#443 **61d open**
- RED: **CLAUDE.md fragments** 9d stale, **day 4** — Wren + Kade escalated, no refresh yet
- RED: **Domain context** 8d stale, **day 4** — 3 more Chorus cards landed yesterday with no update
- RED: **CSC** 152 `/tmp/` in `platform/scripts/` (+3 from yesterday's merges, worsening)

**QUALITY:** RED (Kade, 2026-08-04)
- 0 tests run — all 4 suites blocked: `ts-jest` preset missing, **day 54**; lint blocked, **day 56**
- Build: 181 type errors (+0) — eleventh consecutive stable day, no new regression
- NEW: `platform/mcp-server` — 22 suites, 0 tests run; ESM import error, separate from ts-jest blockage
- Root cause: `npm ci` at repo root — **56 days unresolved, no owner**

**YESTERDAY (08-03):** 5 cards — #3718/#3724 (wren), #3728/#3737 (silas), #3734 (kade)
- CSC +3 `/tmp/` introduced by merges; hooks regression (registration_json re-surfaced)

**TODAY:**
1. **Jeff → assign `npm ci` owner:** Day 56, no ship date — every suite is dark; mcp-server now separately failing
2. **Wren + Kade → fragments:** Day 4 RED — refresh `designing/claudemd/` today, no further slip
3. **Silas → domain context:** Day 4 RED — update `domain-context-chorus.md` and siblings
4. **Silas → CSC regression:** Identify which of yesterday's 5 cards added +3 `/tmp/` paths; fix or offset
5. **Silas → mcp-server ESM:** 22 suites new-failing today; ESM config fix needed

**BLOCKERS (needs Jeff):**
- **`npm ci` day 56** — no owner, no ship date; all suites dark + mcp-server now separately failing
- **Dependabot #449/#443 at 61d** — stalled; call needed: merge or close
