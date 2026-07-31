# Daily Morning Summary — 2026-07-31

**HEADLINE:** Domain context expired today and CLAUDE.md fragments cross 7d — Silas must update chorus context now; `npm ci` hits day 52 with quality fully dark.

**OPS:** YELLOW/RED (Silas, 2026-07-30)
- GREEN: Git clean (0 uncommitted changes)
- YELLOW: 2 dead-code warnings carry (`registration_json`, `owes_response_block`); 2 Dependabot PRs at 57d open (#449 cucumber 12→13, #443 ureq 2→3, auto-rebase disabled); CLAUDE.md fragments 6d — **cross 7d threshold today**
- RED: **Domain context — all 5 files expired today (Jul 31)**; chorus context ~100d stale
- RED: CSC compliance — `/tmp/` refs in `platform/scripts/` up to **149 (+7 from Jul 29)**; remediation card needed

**QUALITY:** RED (Kade, 2026-07-31)
- All 4 suites blocked: `ts-jest` preset not found — **day 50**; lint blocked (`@eslint/js`) — **day 52**
- Build: **181 type errors (+0)** — seventh consecutive stable day, no regression
- Primary fix: `npm ci` at repo root — **52 days unresolved**

**YESTERDAY (07-30):** 4 cards shipped
- **#3713 (silas):** Silas card
- **#3714 (silas):** Silas card
- **#3710 (kade):** Kade card
- **#3716 (silas):** Silas card

**TODAY:**
1. **Silas → domain context:** Breached today — update `domain-context-chorus.md` (100d stale) immediately
2. **Wren + Kade → CLAUDE.md fragments:** Cross 7d today; role content refresh needed now
3. **Silas → CSC /tmp:** +7 overnight; find and stop the source; open remediation card
4. **Jeff → `npm ci`:** Day 52; quality blind since early June — needs an owner and ship date
5. **Jeff/Silas → Dependabot #449/#443:** 57d open; #443 needs rebase re-enabled

**BLOCKERS (needs Jeff):**
- **`npm ci` day 52** — all suites and lint dark; coverage unknown 7+ weeks; no owner assigned
- **Dependabot #449/#443 at 57d** — major breaking upgrades stalled; #443 auto-rebase disabled
