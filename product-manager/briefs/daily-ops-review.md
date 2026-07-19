# Daily Ops Review — 2026-07-19

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 28.97s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, no change)**
17 plist files in `proving/config/launchagents/` reference `/tmp/`. Count unchanged.
**Action:** Silas — migration card to `~/Library/Logs/Chorus/` still open; 17 is canonical.

## 3. CLAUDE.md Fragments
**Status: YELLOW (11d, escalating)**
`designing/claudemd/` last committed 2026-07-08 (11d stale, 4d over threshold). #3658, #3542 shipped today — no fragment refresh.
**Action:** Wren — URGENT; drift risk rising daily; audit this session.

## 4. CSC Compliance
**Status: RED (carry, 36 sh-only)**
36 `.sh` files in `platform/scripts/` contain `/tmp/` refs. Count unchanged.
**Action:** Silas — July scoped card for `platform/scripts/*.sh` still open.

## 5. Git Dirty State
**Status: GREEN**
`gathering-team` repo clean — 0 uncommitted changes.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 103d)**
#1759 and #1791 now 103d with no commits. Wren backlog WIP cards last touched 2026-07-04 (15d). #3607 log rotation still broken live.
**Action:** Wren — #1759/#1791 must close or archive; escalate #3607 to Jeff.

## 7. Domain Context Freshness
**Status: YELLOW (10d, 4 domains)**
chorus/infra/music/seeds all last committed 2026-07-09 (10d, 3d over threshold). #3658 (chorus) shipped today; domain-context-chorus.md not refreshed. Photos GREEN (5d).
**Action:** Silas — domain-context-chorus.md urgent given #3658 today; Wren — music/seeds.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON in `data/`; cross-session delta not computable.
**Action:** Silas — land nightly baseline JSON to `data/` to enable tracking.

---
*Carries: §2 YELLOW (17 plists), §4 RED (36 sh), §6 RED (103d), §7 YELLOW (10d/4 domains), §8 N/A. Escalations: §3 now 11d/4d-over (was 10d); §6 +1d to 103d WIP + 15d Wren backlog; §7 chorus now 10d despite #3658 shipping today.*
