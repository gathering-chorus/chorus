# Daily Ops Review — 2026-08-03

## 1. Hooks Health
**Status: YELLOW (regression)**
`cargo check` passes but warns on 2 dead-code paths — **up from 1 yesterday**. `registration_json` (session_registry.rs:66) returned after 6-card merge batch; `owes_response_block` (nudge_drain.rs:178) still unresolved.
**Action:** Silas — remove both dead-code paths; `registration_json` is the new regression from yesterday's land.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
33 `/tmp/` refs across 17 plist files in `proving/config/launchagents/`. Count unchanged from Jul 30–Aug 2.
**Action:** Silas — migration card open; no movement.

## 3. CLAUDE.md Fragments
**Status: RED (day 4)**
`designing/claudemd/` last committed 2026-07-25 — **9d stale** (threshold 7d). No refresh after 6 cards merged yesterday.
**Action:** Wren + Kade — fragment refresh escalated; overdue since day 3.

## 4. CSC Compliance
**Status: RED (worsening)**
`platform/scripts/` has **152 `/tmp/`** occurrences — up **+3 from 149 yesterday**. Regression introduced by yesterday's card merges.
**Action:** Silas — identify which of yesterday's 6 cards added new `/tmp/` paths and fix or offset.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes across all tracked role and platform directories.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW**
Kade #3721 WIP, last updated 2026-08-02 (~1d) — not yet >48h. Dependabot #449/#443 now **61d open** (up from 60d). Board snapshot stale (Apr 2026).
**Action:** Jeff — call on Dependabot PRs (61d, merge or close); Silas — refresh board snapshot.

## 7. Domain Context Freshness
**Status: RED (day 4)**
All 5 domain-context files last committed 2026-07-26 — **8d stale**. Cards #3722, #3723, #3735 landed yesterday in chorus/wren domain with no context refresh.
**Action:** Silas — update `domain-context-chorus.md`; Wren — assess `domain-context-seeds.md` and others.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` in repo; macOS `diskutil` not runnable in remote env.
**Action:** Silas — land nightly baseline JSON to enable delta tracking.

---
*Aug 3 delta vs Aug 2: §1 hooks regressed 1→2 warnings (`registration_json` back after 6-card land); §3 fragments day 4 RED (no refresh); §4 CSC +3 occurrences (152 from 149); §7 domain context day 4 RED, 3 more chorus cards landed with no update; §6 Dependabot ticks to 61d. Git clean.*
