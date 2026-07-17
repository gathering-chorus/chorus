# Daily Ops Review — 2026-07-17

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 27.29s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, no change)**
17 plist files in `proving/config/launchagents/` reference `/tmp/`. Count unchanged; migration card still open.
**Action:** Silas — migration card to `~/Library/Logs/Chorus/` still open; 17 is canonical.

## 3. CLAUDE.md Fragments
**Status: YELLOW (9d, escalating)**
`designing/claudemd/` last committed 2026-07-08 (9d stale, 2d over threshold). No refreshes despite Chorus shipping #3653, #3658, #3656, #3628, #3651 this week.
**Action:** Wren — escalating; audit for drift before next session close.

## 4. CSC Compliance
**Status: RED (carry, 36 sh-only)**
36 `.sh` files in `platform/scripts/` contain `/tmp/` refs. Count unchanged from yesterday. (`messages/scripts/` and `architect/scripts/` paths don't exist here.)
**Action:** Silas — July scoped card for `platform/scripts/*.sh` still open.

## 5. Git Dirty State
**Status: GREEN**
`gathering-team` repo clean — 0 uncommitted. 5 external role repos not cloned in this environment.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 101d + Wren backlog 13d)**
#1759 and #1791 now 101d with no commits. Wren next-session shows 10 WIP cards last touched 2026-07-04 (13d ago); session note flags #3607 rotation still broken live (chorus.log at 122MB, fix awaiting Jeff approve).
**Action:** Wren — close or re-groom #1759/#1791; unblock #3607 rotation fix.

## 7. Domain Context Freshness
**Status: YELLOW (9d, 4 domains)**
chorus/infra/music/seeds domain-context files last committed 2026-07-08 (9d, 2d over threshold). Chorus shipped 5+ cards this week; infra touched. `domain-context-photos.md` still GREEN.
**Action:** Silas — chorus/infra most urgent; Wren — music/seeds.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON in `data/`; cross-session delta not computable.
**Action:** Silas — land nightly baseline JSON to `data/` to enable tracking.

---
*Carries: §2 YELLOW (17 plists), §4 RED (36 sh), §6 RED (101d), §7 YELLOW (9d/4 domains), §8 N/A. New: §3 escalated to 9d/2d-over; §6 adds Wren 13d backlog + #3607 rotation broken.*
