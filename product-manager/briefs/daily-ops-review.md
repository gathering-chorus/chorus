# Daily Ops Review — 2026-08-02

## 1. Hooks Health
**Status: YELLOW (improving)**
`cargo check` passes. Dead-code warnings down 2→1: `owes_response_block` (nudge_drain.rs:178) remains; `registration_json` resolved since Aug 1.
**Action:** Silas — remove or suppress remaining dead-code path.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
33 `/tmp/` refs across 17 plist files in `proving/config/launchagents/`. Count unchanged from Jul 30–Aug 1.
**Action:** Silas — migration card open; no movement.

## 3. CLAUDE.md Fragments
**Status: RED (day 3)**
`designing/claudemd/` last committed 2026-07-25 — now **9d stale** (threshold 7d). No refresh in today's commits (#3706, #3720, quality review).
**Action:** Wren + Kade — role content refresh required today.

## 4. CSC Compliance
**Status: RED (carry)**
`platform/scripts/` has **149 `/tmp/`** occurrences — count unchanged from Jul 30–Aug 1. Prompt paths `messages/scripts/` / `architect/scripts/` still don't exist.
**Action:** Silas — no movement; update prompt paths to `platform/scripts/`.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes across all tracked role and platform directories.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry)**
Board snapshot stale (Apr 2026). Dependabot PRs #449 and #443 now at **60d open** (up from 59d Aug 1).
**Action:** Jeff/Silas — close or merge Dependabot PRs; refresh board snapshot.

## 7. Domain Context Freshness
**Status: RED (day 3)**
All 5 domain-context files last committed 2026-07-25 — **9d stale**. 4 cards shipped this week in Chorus domain (#3706, #3719, #3720 wren/silas; #3717 kade) with no context refresh.
**Action:** Silas — update `domain-context-chorus.md`; Wren to assess others.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` in repo; script targets macOS `diskutil`, not runnable in remote env.
**Action:** Silas — land nightly baseline JSON to enable delta tracking.

---
*Aug 2 delta: §1 hooks improved (2→1 warning, `registration_json` resolved). §3 fragments now 9d (day 3 RED). §7 domain context 9d stale, 4 cards shipped in Chorus this week with no update (day 3 RED). §6 Dependabot PRs tick to 60d. All /tmp counts held.*
