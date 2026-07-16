# Daily Ops Review — 2026-07-16

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 29.43s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, no change)**
17 plist files in `proving/config/launchagents/` reference `/tmp/`. Count unchanged from yesterday.
**Action:** Silas — migration card to `~/Library/Logs/Chorus/` still open; 17 is canonical.

## 3. CLAUDE.md Fragments
**Status: YELLOW (8d)**
`designing/claudemd/` last committed 2026-07-08 (8d stale, 1d over threshold). No refreshes despite continued card shipping.
**Action:** Wren — audit for drift; escalating.

## 4. CSC Compliance
**Status: RED (carry, 36 sh-only)**
36 `.sh` files in `platform/scripts/` contain `/tmp/` refs. `messages/scripts/` and `architect/scripts/` paths do not exist in this repo. Count unchanged.
**Action:** Silas — July scoped card for `platform/scripts/*.sh` still open.

## 5. Git Dirty State
**Status: GREEN**
`gathering-team` repo clean — 0 uncommitted. 5 expected role repos (engineer, messages, jeff-bridwell-personal-site, shared-observability, wordpress-blog) not cloned in this environment; only product-manager and architect checked.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 100d)**
#1759 "OWL entity model" and #1791 "Restore chorus product boundary" hit 100d stale today — no commit activity on either. No other WIP untouched >48h found in role state.
**Action:** Wren — close or re-groom both today; milestone crossed.

## 7. Domain Context Freshness
**Status: YELLOW (8d, 4 domains)**
`domain-context-photos.md` last updated 2026-07-14 — GREEN. Chorus/infra/music/seeds last committed 2026-07-08 (8d, 1d over threshold). Chorus shipped 5+ cards this week (#3653, #3658, #3628 etc.); infra touched (#3541).
**Action:** Silas — chorus/infra context most urgent; Wren — music/seeds.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON in `data/`; cross-session delta not computable.
**Action:** Silas — land nightly baseline JSON to `data/` to enable tracking.

---
*New: §3 date corrected (was 12d/Jul-03, now 8d/Jul-08 per fresh git log); §6 hits 100d milestone. Carries: §2 YELLOW (17), §4 RED (sh-36), §7 YELLOW (4 domains 8d). §1/§5 green.*
