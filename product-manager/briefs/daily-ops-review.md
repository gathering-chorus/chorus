# Daily Ops Review — 2026-08-07

## 1. Hooks Health
**Status: YELLOW (carry, day 5)**
`cargo check` passes with 2 dead-code warnings — unchanged since Aug 2. `registration_json` (session_registry.rs:66) and `owes_response_block` (nudge_drain.rs:178) still unresolved.
**Action:** Silas — remove both dead-code paths; fifth consecutive carry.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17+ plist files with `/tmp` refs in `proving/config/launchagents/` — unchanged. `com.chorus.hooks.plist`, `com.chorus.api.plist`, `com.chorus.clearing.plist` among active plists still logging to `/tmp`. No migration progress.
**Action:** Silas — migration card stalled >8 days; confirm card exists and assign.

## 3. CLAUDE.md Fragments
**Status: RED (day 8, threshold breached)**
Domain-context files and most claudemd fragments last committed Jul 30. Only `tdd-discipline.md` refreshed Aug 3 via #3734. Morning summary confirms "8d breach." Threshold (7d) passed yesterday; no update landed today.
**Action:** Wren + Silas — `domain-context-chorus.md` (Silas), `domain-context-seeds.md` + remaining shared/ fragments (Wren); urgent.

## 4. CSC Compliance
**Status: RED (improving)**
124 `/tmp/` occurrences in `platform/scripts/` (non-plist) — down from 148 (Aug 5). Active reduction underway; 68 files still affected. `messages/scripts/` and `architect/scripts/` not present in this repo.
**Action:** Silas — track +24 reduction since Aug 5; open card to drive to zero, identify which script batch landed the drop.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes in repo. Active shipping today: #3780, #3781, #3782 (Wren), #3773 (Silas). Other role repos not accessible from this environment.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW (carry)**
Dependabot #449/#443 now **65d open** (+2d). Active card velocity good (5 cards merged today, 9 Aug 6). No GitHub API access to enumerate open WIP — carrying stale-bot state forward.
**Action:** Jeff — #449/#443 decision at 65d; Silas — confirm Aug 1 Dependabot batch (6d, no owner).

## 7. Domain Context Freshness
**Status: RED (day 8)**
All 5 domain-context files last committed Jul 30 (8 days). Active shipping in chorus (#3771, #3773) and seeds domains with no context refresh. Breach now 1d past threshold.
**Action:** Silas — `domain-context-chorus.md`; Wren — `domain-context-seeds.md`, `domain-context-infrastructure.md`, `domain-context-music.md`, `domain-context-photos.md`. Block ships until refreshed.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` committed; `platform/scripts/perf-baseline.sh` exists but produces no committed artifacts. Delta tracking not possible.
**Action:** Silas — land nightly baseline JSON output to enable comparison.

---
*Aug 7 delta vs Aug 5: §1 hooks 2 warnings unchanged (day 5). §3 fragments hit 8d RED threshold breached. §4 CSC 124 occurrences (was 148 — down 24, methodology comparable). §6 Dependabot 65d (+2d, no decision). §7 domain context day 8 RED, chorus+seeds cards shipped with no refresh. All other checks unchanged.*
