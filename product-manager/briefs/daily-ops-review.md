# Daily Ops Review — 2026-07-20

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 40.52s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, no change)**
17 plist files in `proving/config/launchagents/` reference `/tmp/` (count unchanged). Additional 2 in `platform/scripts/launchagents-secondary/`.
**Action:** Silas — migration card to `~/Library/Logs/Chorus/` still open; 17+2 tracked.

## 3. CLAUDE.md Fragments
**Status: YELLOW (8d stale)**
`designing/claudemd/` last committed 2026-07-12 via #3634 — 8d ago, 1d over 7d threshold. Note: yesterday's brief cited Jul 8; git head shows Jul 12.
**Action:** Wren — refresh claudemd fragments this session; risk rising.

## 4. CSC Compliance
**Status: RED (carry, 36 sh-only)**
36 `.sh` files in `platform/scripts/` contain `/tmp/` refs. Count unchanged.
**Action:** Silas — July scoped card for `platform/scripts/*.sh` still open.

## 5. Git Dirty State
**Status: GREEN**
`gathering-team` repo clean — 0 uncommitted changes.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 104d)**
No live Vikunja board access; carrying yesterday: #1759/#1791 now 104d without commits. Wren backlog WIP last touched 2026-07-04 (16d).
**Action:** Wren — #1759/#1791 must close or archive; escalate #3607 to Jeff.

## 7. Domain Context Freshness
**Status: YELLOW (8d, 4 domains)**
chorus/infra/music/seeds last committed 2026-07-12 (8d, 1d over threshold). #3661/#3657 (kade, 2026-07-17) shipped werk-test+api in chorus domain — domain-context-chorus.md still not refreshed. Photos GREEN (6d, Jul 14).
**Action:** Silas — domain-context-chorus.md urgent given #3661/#3657; Wren — music/seeds.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON in `data/`; cross-session delta not computable.
**Action:** Silas — land nightly baseline JSON to `data/` to enable tracking.

---
*Carries: §2 YELLOW (17+2 plists), §4 RED (36 sh), §6 RED (104d), §7 YELLOW (8d/4 domains), §8 N/A. Escalations: §3 discrepancy resolved to 8d/Jul 12; §6 +1d to 104d WIP + 16d Wren backlog; §7 chorus 8d despite #3661/#3657 shipping Jul 17.*
