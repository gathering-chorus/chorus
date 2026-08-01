# Daily Morning Summary — 2026-08-01

**HEADLINE:** CLAUDE.md fragments and domain context both expired — must refresh today; `npm ci` hits day 53 with quality still fully dark.

**OPS:** YELLOW/RED (Silas, 2026-07-31)
- GREEN: Git clean (0 uncommitted changes)
- YELLOW: 2 dead-code warnings carry (`registration_json`, `owes_response_block`); LaunchAgent `/tmp/` refs carry (33 refs, 17 plist files, migration card open); Dependabot #449/#443 at **58d open** (#443 auto-rebase disabled)
- RED: **CLAUDE.md fragments — 7d threshold breached; actual path is `designing/claudemd/`, not `messages/claudemd/`**; all fragments last committed Jul 25
- RED: **Domain context — all 5 files expired (last Jul 25, 7d threshold hit); chorus context ~100d stale**; 4 cards shipped Jul 31 with no context update
- RED: CSC compliance — 149 `/tmp/` refs in `platform/scripts/`; prompt paths incorrect

**QUALITY:** RED (Kade, 2026-08-01)
- All 4 suites blocked: `ts-jest` preset not found — **day 51**; lint blocked (`@eslint/js`) — **day 53**
- Build: **181 type errors (+0)** — eighth consecutive stable day; debt from Jul 25/26 accumulation unchanged
- Primary fix: `npm ci` at repo root — **53 days unresolved**

**YESTERDAY (07-31):** 2 cards shipped
- **#3717 (kade):** Pinned Rust toolchain to 1.97.1 + clippy ratchet fix — 15 clippy violations resolved; local/CI now agree
- **#3689 (silas):** OIDC auth overhaul (chorus-oidc src, owl-api coverage, security scopes TTL); co-authored with Kade

**TODAY:**
1. **Silas → domain context:** Overdue — update `domain-context-chorus.md` immediately (100d stale, formally expired)
2. **Wren + Kade → CLAUDE.md fragments:** Fix prompt path to `designing/claudemd/`; refresh all role content
3. **Silas → ops prompt paths:** Correct §3 (`designing/claudemd/`) and §4 (`platform/scripts/`) in ops review prompt
4. **Jeff → `npm ci`:** Day 53; all suites and lint dark — needs owner and ship date this week
5. **Jeff/Silas → Dependabot #449/#443:** 58d stalled; #443 needs rebase re-enabled before any merge

**BLOCKERS (needs Jeff):**
- **`npm ci` day 53** — quality fully blind since early June; no owner, no ship date
- **Dependabot #449/#443 at 58d** — breaking upgrades stalled; #443 auto-rebase disabled blocks merge
