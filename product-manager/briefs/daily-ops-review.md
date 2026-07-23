# Daily Ops Review — 2026-07-23

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 28.42s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, 17+2)**
17 plists in `proving/config/launchagents/` + 2 in `platform/scripts/launchagents-secondary/` reference `/tmp/`. Count unchanged from Jul 21.
**Action:** Silas — migration card to `~/Library/Logs/Chorus/` still open; track at 17+2.

## 3. CLAUDE.md Fragments
**Status: YELLOW (11d stale)**
`designing/claudemd/` last updated Jul 12 per previous baseline; git log confirms no content commit since Jul 14 ops review. 11d stale, 4d over 7d threshold.
**Action:** Wren — claudemd refresh overdue; escalate if not updated by EOD Jul 23.

## 4. CSC Compliance
**Status: RED (carry, 37 sh +1)**
37 `.sh` files in `platform/scripts/` contain `/tmp/` refs — up from 36 (+1 net new). `messages/scripts/` and `architect/scripts/` paths absent, scoped check clean.
**Action:** Silas — count grew +1 since Jul 21; identify new file, open targeted card.

## 5. Git Dirty State
**Status: GREEN**
All 7 role directories clean — 0 uncommitted changes across product-manager, architect, engineer, messages, jeff-bridwell-personal-site, shared-observability, wordpress-blog.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 107d)**
No live Vikunja board access; carrying: #1759/#1791 now 107d without commits (+2d). Wren backlog WIP last touched 2026-07-04 (19d, +2d).
**Action:** Wren — #1759/#1791 must close or archive; escalate #3607 to Jeff.

## 7. Domain Context Freshness
**Status: RED (9d, cards shipped)**
All 5 domain-context files last committed Jul 14 (9d ago). 7 cards shipped Jul 14–23: #3611 (silas), #3657/#3660/#3661/#3662/#3663 (kade). chorus and infrastructure domains have shipped cards since last refresh; domain-context-chorus.md and domain-context-infrastructure.md now critical.
**Action:** Silas/Kade — refresh chorus + infra contexts; Wren — music/seeds.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON in `data/`; cross-session delta not computable.
**Action:** Silas — land nightly baseline JSON to `data/` to enable tracking.

---
*Carries: §2 YELLOW (17+2 plists), §6 RED (107d WIP, backlog 19d). Changes: §4 RED escalated (+1 sh file, 37 total); §7 escalated to RED (9d stale, 7 cards shipped since refresh). §3 now 11d/4d over threshold.*
