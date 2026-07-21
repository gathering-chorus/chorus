# Daily Ops Review — 2026-07-21

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 27.82s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, 17+2)**
17 plist files in `proving/config/launchagents/` reference `/tmp/` (count unchanged). Additional 2 in `platform/scripts/launchagents-secondary/`. `platform/scripts/` plists verified clean today.
**Action:** Silas — migration card to `~/Library/Logs/Chorus/` still open; 17+2 tracked.

## 3. CLAUDE.md Fragments
**Status: YELLOW (9d stale)**
`designing/claudemd/` present (build 217); last confirmed commit 2026-07-12 via #3634 — 9d ago. Git history not navigable in cloned session; carrying Jul 12 baseline. Web session shows no fragment update commit since Jul 17 kade cards.
**Action:** Wren — claudemd refresh urgent; 9d stale, 2d over threshold.

## 4. CSC Compliance
**Status: RED (carry, 36 sh-only)**
36 `.sh` files in `platform/scripts/` contain `/tmp/` refs. Count confirmed unchanged today. `messages/scripts/` and `architect/scripts/` do not exist — scoped check clean.
**Action:** Silas — July scoped card for `platform/scripts/*.sh` still open.

## 5. Git Dirty State
**Status: GREEN**
All 7 role directories clean — 0 uncommitted changes across product-manager, architect, engineer, messages, jeff-bridwell-personal-site, shared-observability, wordpress-blog.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 105d)**
No live Vikunja board access; carrying: #1759/#1791 now 105d without commits (+1d). Wren backlog WIP last touched 2026-07-04 (17d, +1d).
**Action:** Wren — #1759/#1791 must close or archive; escalate #3607 to Jeff.

## 7. Domain Context Freshness
**Status: YELLOW (9d, 4 domains)**
chorus/infra/music/seeds last confirmed commit 2026-07-12 — 9d ago (file mtime appears 4d from clone; git commit date takes precedence). #3661/#3657 (kade, 2026-07-17) shipped in chorus domain — domain-context-chorus.md still not refreshed per git log.
**Action:** Silas — domain-context-chorus.md now critical (9d, cards shipped 4d ago); Wren — music/seeds.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON in `data/`; `proving/data/` absent. Cross-session delta not computable.
**Action:** Silas — land nightly baseline JSON to `data/` to enable tracking.

---
*Carries: §2 YELLOW (17+2 plists), §4 RED (36 sh), §6 RED (105d WIP +1d, backlog 17d), §7 YELLOW (9d/4 domains), §8 N/A. New: §3 escalated to 9d/2d over threshold. §7 chorus now critical — 9d gap with cards shipped Jul 17.*
