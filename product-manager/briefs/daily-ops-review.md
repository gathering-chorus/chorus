# Daily Ops Review — 2026-07-18

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 29.13s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, no change)**
17 plist files in `proving/config/launchagents/` reference `/tmp/`. Count unchanged; migration card still open.
**Action:** Silas — migration card to `~/Library/Logs/Chorus/` still open; 17 is canonical.

## 3. CLAUDE.md Fragments
**Status: YELLOW (10d, escalating)**
`designing/claudemd/` last committed 2026-07-08 (10d stale, 3d over threshold). Two more cards shipped today (#3661, #3657 kade). No fragment refresh in 10 days.
**Action:** Wren — URGENT; audit for drift this session; 2 new kade cards add surface area.

## 4. CSC Compliance
**Status: RED (carry, 36 sh-only)**
36 `.sh` files in `platform/scripts/` contain `/tmp/` refs. Count unchanged.
**Action:** Silas — July scoped card for `platform/scripts/*.sh` still open.

## 5. Git Dirty State
**Status: GREEN**
`gathering-team` repo clean — 0 uncommitted changes.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 102d)**
#1759 and #1791 now 102d with no commits. Wren backlog WIP cards last touched 2026-07-04 (14d). #3607 log rotation still broken live (chorus.log ~122MB+, awaiting Jeff approve).
**Action:** Wren — #1759/#1791 must close or archive this session; escalate #3607 to Jeff.

## 7. Domain Context Freshness
**Status: YELLOW (9d, 4 domains)**
chorus/infra/music/seeds last committed 2026-07-09 (9d, 2d over threshold). Chorus shipped #3661/#3657 today; domain-context-chorus.md not refreshed. `domain-context-photos.md` GREEN (4d).
**Action:** Silas — domain-context-chorus.md most urgent given today's shipments; Wren — music/seeds.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON in `data/`; cross-session delta not computable.
**Action:** Silas — land nightly baseline JSON to `data/` to enable tracking.

---
*Carries: §2 YELLOW (17 plists), §4 RED (36 sh), §6 RED (102d), §7 YELLOW (9d/4 domains), §8 N/A. Escalations: §3 now 10d/3d-over; §6 +1d to 14d Wren backlog; §7 chorus domain stale despite today's kade shipments.*
