# Daily Ops Review — 2026-07-27

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes clean. Same 2 dead-code warnings (4 lines): `owes_response_block` (nudge_drain.rs:178) and `registration_json` (session_registry.rs:66). No regression from Jul 26.
**Action:** Silas — suppress or remove dead-code; no movement since first flagged.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17 plists in `proving/config/launchagents/` + 2 in `platform/scripts/launchagents-secondary/` reference `/tmp/`. Count unchanged from Jul 26.
**Action:** Silas — migration card still open; no movement.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry)**
Role-specific fragments (roles/wren, silas, kade) and shared fragments last committed 2026-07-23. No updates since kade #3288 (Jul 25). Now 4d without a fragment update while 4 cards shipped today.
**Action:** Wren + Kade — role fragments still need content refresh; 4-card delta since last touch.

## 4. CSC Compliance
**Status: RED (carry)**
`messages/scripts/` and `architect/scripts/` don't exist in this repo. `platform/scripts/` has **37 .sh files** with `/tmp/` refs — count unchanged from Jul 26.
**Action:** Silas — targeted remediation card still needed; check-path mismatch is a separate tracking gap.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes. 4 cards landed today: #3699 + #3697 (Wren — OWL/chorus), #3698 (Wren), #3700 (Silas). Strong velocity.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry)**
Board snapshots contain 2 WIP cards from 2026-04-07 (111d stale). Snapshots last refreshed to git 2026-07-23 but contain April data — Vikunja not queryable from session.
**Action:** Wren — #1759/#1791 must close or archive; board snapshot requires a live refresh.

## 7. Domain Context Freshness
**Status: RED (carry +4 cards)**
All 5 context files at 2026-07-23 (4d). 4 cards shipped today (#3699/#3698/#3700/#3697) spanning chorus and infrastructure — `domain-context-chorus.md` content now ~99d stale with 6+ cards landed since last update.
**Action:** Silas — chorus context critical; Wren — seeds/music/photos still past threshold. All 5 overdue.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` captured in `proving/logs/`. Scripts present but produce no persisted output accessible in session.
**Action:** Silas — land nightly baseline JSON to `proving/logs/` to enable delta tracking.

---
*Carries unchanged: §1 YELLOW (2 dead-code), §2 YELLOW (19 plists), §4 RED (37 sh), §6 RED (111d WIP snapshots), §8 N/A. §5 GREEN — strong Jul 27 velocity (4 cards). Escalation: §7 now ~99d stale with 4 more chorus cards shipped today — update critical.*
