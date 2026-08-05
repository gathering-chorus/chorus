# Daily Ops Review — 2026-08-05

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes with 2 dead-code warnings — unchanged since Aug 2. `registration_json` (session_registry.rs:66) and `owes_response_block` (nudge_drain.rs:178) still unresolved. No movement.
**Action:** Silas — remove both dead-code paths; third consecutive carry.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17 plist files with `/tmp` refs in `proving/config/launchagents/` — unchanged. chorus-hooks plist hardcodes `/tmp/chorus-hooks.stdout.log` and `/tmp/chorus-hooks.stderr.log`. No migration progress in 6 days.
**Action:** Silas — migration card stalled since Jul 30; open or assign.

## 3. CLAUDE.md Fragments
**Status: YELLOW (partial, day 6)**
Most `designing/claudemd/shared/` fragments last committed Jul 30 (6 days). Only `tdd-discipline.md` refreshed Aug 3 via #3734. 23 of 24 shared fragments approaching 7-day threshold.
**Action:** Wren — bulk-refresh remaining shared/ fragments; threshold breached tomorrow.

## 4. CSC Compliance
**Status: RED (improving)**
`platform/scripts/` has 148 `/tmp/` occurrences — down 4 from yesterday (was 152). `messages/scripts/` and `architect/scripts/` absent from this repo. Positive delta; still extensive.
**Action:** Silas — track the -4 reduction; open card to drive to zero.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes in repo. No other role repos accessible from this environment.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW (carry)**
Dependabot #449/#443 now **63d open**. Aug 1 batch (10 total open bot PRs) now 4d old. GitHub MCP scope restriction prevented live count — carrying yesterday's state. No new card commits today (Aug 5) yet.
**Action:** Jeff — #449/#443 decision at 63d; Silas — triage Aug 1 batch (4d, no owner).

## 7. Domain Context Freshness
**Status: RED (day 6)**
All 5 domain-context files last committed Jul 30 (6 days). Active shipping in chorus/identity domains Aug 4: #3736, #3743, #3745, #3747, #3750 — no context refresh. Sixth consecutive day RED.
**Action:** Silas — `domain-context-chorus.md`; Wren — `domain-context-seeds.md` and others. Urgent: threshold breach tomorrow.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` committed; scripts exist (`platform/scripts/perf-baseline.sh`) but produce no committed artifacts for delta tracking.
**Action:** Silas — land nightly baseline JSON output to enable delta comparison.

---
*Aug 5 delta vs Aug 4: §4 CSC -4 occurrences (148←152); §6 Dependabot 63d (+1d, no decision); §7 domain context day 6 RED, #3736/#3743/#3745/#3747/#3750 landed Aug 4 with no refresh. §3 fragments hit 6d today — threshold breach tomorrow. All other checks unchanged.*
