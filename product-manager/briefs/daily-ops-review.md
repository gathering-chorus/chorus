# Daily Ops Review — 2026-08-08

## 1. Hooks Health
**Status: YELLOW (day 6 carry)**
`cargo check` passes; 2 dead-code warnings unchanged since Aug 2. `registration_json` (session_registry.rs:66) and `owes_response_block` (nudge_drain.rs:178).
**Action:** Silas — remove both dead-code paths; sixth consecutive carry.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17+ plist files still logging to `/tmp` (hooks, api, clearing, ops, context-cache). No migration progress.
**Action:** Silas — confirm migration card exists and assign; >8d stall.

## 3. CLAUDE.md Fragments
**Status: GREEN (resolved from RED)**
Last git commit for `designing/claudemd/` and `designing/domain-context/` was Aug 3 (#3718) — 5 days ago, under 7d threshold. Yesterday's RED was carrying an incorrect Jul 30 date.
**Action:** None; monitor — 2d until threshold re-breach.

## 4. CSC Compliance
**Status: RED (regression)**
156 `/tmp/` occurrences in `platform/scripts/` — up from 124 yesterday (+32). `chorus-model-deploy.sh` (+16, now 16 refs) is the primary driver added in recent #3785–#3788 batch. `messages/scripts/` and `architect/scripts/` not present in this repo.
**Action:** Silas — audit `chorus-model-deploy.sh`; 32-hit regression reverses prior week's reduction trend.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes. Active shipping today: #3785, #3788, #3790 (Silas). Repo clean.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW (carry)**
Board snapshots dated Apr 7 — stale, can't enumerate live WIP. Dependabot #449/#443 now 67d open (+2d). No GitHub API access to enumerate current WIP list.
**Action:** Jeff — #449/#443 decision at 67d; Silas — Dependabot batch disposition.

## 7. Domain Context Freshness
**Status: GREEN (resolved from RED)**
All 5 domain-context files last updated Aug 3 (5 days). 50 cards shipped since; chorus domain most active (#3785, #3788, #3790 today). Under 7d threshold.
**Action:** None; monitor — 2d until threshold.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` committed; `platform/scripts/perf-baseline.sh` exists but emits no committed artifacts. Repo size: 690MB.
**Action:** Silas — land nightly baseline JSON to enable delta tracking.

---
*Aug 8 delta vs Aug 7: §3 fragments GREEN (resolved — git log shows Aug 3, not Jul 30). §4 CSC RED regression +32 (156 total); chorus-model-deploy.sh new entries. §7 domain context GREEN (same resolution as §3). §1 hooks day 6 carry. §6 Dependabot 67d. All other checks unchanged.*
