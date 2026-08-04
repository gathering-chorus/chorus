# Daily Ops Review — 2026-08-04

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes with 2 dead-code warnings — unchanged from yesterday. `registration_json` (session_registry.rs:66) and `owes_response_block` (nudge_drain.rs:178) still unresolved.
**Action:** Silas — no movement since Aug 2; remove both dead-code paths.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
33 `/tmp/` refs across 17 plist files in `proving/config/launchagents/` — unchanged from Jul 30–Aug 3.
**Action:** Silas — migration card stalled; no movement for 5 days.

## 3. CLAUDE.md Fragments
**Status: YELLOW (partial)**
`designing/claudemd/` dir touched Aug 3 but shared/ fragments still at 2026-07-27 (8d stale, threshold 7d). 24 fragments in shared/; not fully recovered from RED day 4.
**Action:** Wren — confirm which fragments were refreshed Aug 3; update remaining shared/ fragments.

## 4. CSC Compliance
**Status: RED (carry)**
`platform/scripts/` has 152 `/tmp/` occurrences — unchanged from yesterday. (`messages/scripts/` and `architect/scripts/` absent from repo.)
**Action:** Silas — no progress; assign owner or open card.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes in repo.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW (worsening)**
Dependabot #449/#443 now **62d open** (up from 61d). New surge: **8 Dependabot PRs** opened Aug 1 (#838–#845, 3d) — jest, ts-jest, eslint-plugin-security, setup-node, ureq bumps. Total open bot PRs: 10.
**Action:** Jeff — decision on #449/#443 (62d, merge or close); Silas — triage Aug 1 batch.

## 7. Domain Context Freshness
**Status: RED (day 5)**
All 5 domain-context files last committed ~Jul 27 (8d stale). Cards #3718, #3724, #3728, #3734, #3737 landed Aug 3–4 in chorus/wren domains with no context refresh.
**Action:** Silas — `domain-context-chorus.md`; Wren — `domain-context-seeds.md` and others. Fifth consecutive day RED.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` in repo. Host at 20% capacity (363MB repo, 30GB free of 252GB total).
**Action:** Silas — land nightly baseline JSON to enable delta tracking.

---
*Aug 4 delta vs Aug 3: §3 fragments partial recovery (dir touched Aug 3, shared/ still 8d stale); §6 Dependabot 62d + 8 new PRs from Aug 1 surge; §7 domain context day 5 RED, cards #3718/#3724/#3728/#3734/#3737 landed with no update. All other checks unchanged.*
