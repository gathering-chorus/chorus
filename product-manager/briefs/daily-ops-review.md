# Daily Ops Review — 2026-08-10

## 1. Hooks Health
**Status: YELLOW (day 8 carry)**
`cargo check` passes; 2 dead-code warnings unchanged since Aug 2. `registration_json` (session_registry.rs:66) and `owes_response_block` (nudge_drain.rs:178).
**Action:** Silas — remove both dead-code paths; eighth consecutive carry.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, >10d)**
20+ plist files in `proving/config/launchagents/` logging to `/tmp` (hooks, api, clearing, ops, context-cache, fuseki-perf, posture-capture, launchagent-metrics, harvest-exporter, etc.). No migration progress.
**Action:** Silas — confirm migration card exists and is assigned; >10d stall.

## 3. CLAUDE.md Fragments
**Status: YELLOW (breach imminent)**
`designing/claudemd/` present; no automated diff tooling. Domain-context files last committed Aug 4 (6d ago); 7d threshold = breach tomorrow (Aug 11).
**Action:** Wren — update domain-context-chorus.md and domain-context-infrastructure.md today.

## 4. CSC Compliance
**Status: RED (regression)**
156 `/tmp/` refs in `platform/scripts/` — up from yesterday's reported 96 (+60). Matches the Aug 8 level; unclear if #3805/#3804 introduced regressions or yesterday's measurement was a WD artifact. Same delta pattern as Aug 8 spike.
**Action:** Silas — recount clean from repo root; identify source of regression if confirmed.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes. Today's shipped cards: #3805 (guard URLs, Silas), #3804 (llug.com/chorus guard, Silas). Repo clean.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW (carry)**
All 10 open PRs are Dependabot. #449 (cucumber 13.0.0) and #443 (ureq 3.0) now 69d open. New Dependabot batch #838–#845 opened Aug 1 (9d stale). No human WIP cards in PR state.
**Action:** Jeff — decision on #449/#443 at 69d; Silas — batch-disposition #838–#845 before they age further.

## 7. Domain Context Freshness
**Status: YELLOW (breach imminent)**
All 5 domain-context files (chorus, infrastructure, music, photos, seeds) last committed Aug 4 (6d). Active domains this week: chorus (#3804, #3805). Breach at 7d = tomorrow (Aug 11) if no update.
**Action:** Wren — update domain-context-chorus.md today; infrastructure secondary.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` committed; `platform/scripts/perf-baseline.sh` exists but emits no committed artifacts. Cannot compute delta.
**Action:** Silas — land nightly baseline JSON to enable delta tracking.

---
*Aug 10 delta vs Aug 9: §4 CSC RED — 156 (regression from reported 96; possible measurement artifact). §7 domain context YELLOW (6d, breach tomorrow). §3 fragments YELLOW same. §1 hooks day 8 carry. §6 Dependabot #449/#443 69d (+1d), new batch #838–#845 at 9d. §5 clean. All other checks carry.*
