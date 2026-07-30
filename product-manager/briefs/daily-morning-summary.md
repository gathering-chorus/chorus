# Daily Morning Summary — 2026-07-30

**HEADLINE:** Domain context files expire **tomorrow (Jul 31)** — Silas must update chorus context today; `npm ci` hits day 51 with quality fully dark.

**OPS:** YELLOW/RED (Silas, 2026-07-29)
- GREEN: Git clean (0 uncommitted changes)
- YELLOW: Dead-code warnings **back to 2** (regression — `registration_json` returned); 2 Dependabot PRs at 56d open (#449 cucumber 12→13, #443 ureq 2→3, auto-rebase disabled); CLAUDE.md fragment sync lag (5d+)
- RED: CSC compliance — check-paths still wrong; `/tmp/` refs in `platform/scripts/` corrected to **142 occurrences** (was 37); **all 5 domain context files expire Jul 31** — chorus context ~100d stale, 4 infra cards shipped with no update

**QUALITY:** RED (Kade, 2026-07-30)
- All 4 suites blocked: `ts-jest` preset not found — **day 49**; lint blocked (`@eslint/js`) — **day 51**
- Build: **181 type errors (+0)** — sixth consecutive stable day, no regression
- Primary fix: `npm ci` at repo root — **51 days unresolved**

**YESTERDAY (07-29):** 3 cards shipped
- **#3708 (wren):** Re-pointed test-clearing-ack Test 2 to `jeff-input.ts`; 7/7 green — kills nightly false-red
- **#3709 (silas):** Silas card (details in commit)
- **#3711 (silas):** Silas card (details in commit)

**TODAY:**
1. **Silas → domain context:** All 5 files expire tomorrow — `domain-context-chorus.md` critical at ~100d; update today or set expiry extension
2. **Jeff → `npm ci`:** Day 51; quality blind since early June — owner or go/no-go needed
3. **Jeff/Silas → Dependabot #449/#443:** 56d open; #443 needs rebase re-enabled; major breaking upgrades need a decision
4. **Silas → dead-code regression:** `registration_json` returned; verify whether Jul 24 fix actually landed
5. **Wren + Kade → CLAUDE.md fragments:** Role content refresh overdue (5d+ lag)

**BLOCKERS (needs Jeff):**
- **Domain context expiry Jul 31** — chorus content ~100d stale; goes red tomorrow if not updated
- **`npm ci` day 51** — all suites and lint dark; coverage unknown for 7+ weeks
- **Dependabot #449/#443 at 56d** — major breaking upgrades stalled; #443 auto-rebase disabled
