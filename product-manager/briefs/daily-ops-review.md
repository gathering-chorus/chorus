# Daily Ops Review — 2026-06-13

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes with same 8 dead-code warnings — no regression, no improvement. `chorus_worktree_override` (`types.rs:64`) and `load_role_sections` now at 3-day carry.
**Action:** Silas — resolve or `#[allow(dead_code)]`; carry cost rising.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
17 plists in `proving/config/launchagents/` + `platform/services/chorus-hooks/com.chorus.hooks.plist` use `/tmp` for log paths. Count unchanged from Jun 10.
**Action:** Migrate to `~/Library/Logs/Chorus/`; no movement in 3 days.

## 3. CLAUDE.md Fragments
**Status: GREEN**
`messages/claudemd/` absent; fragments at `designing/claudemd/shared/`. No divergence detectable.
**Action:** None.

## 4. CSC Compliance
**Status: GREEN**
`messages/scripts/` and `architect/scripts/` absent from this repo. No `/tmp/` violations in scope.
**Action:** None.

## 5. Git Dirty State
**Status: GREEN**
`product-manager` clean (0 dirty files). Other 6 role dirs live in separate repos outside this worktree — verify locally if needed.
**Action:** None in scope.

## 6. Stale WIP Cards
**Status: GREEN**
10+ commits in last 24h: wren (#3373, #3378, #3365), silas (#3370, #3380, #3379, #3369), kade (#3376, #3305, #3375). No gap >48h.
**Action:** None.

## 7. Domain Context Freshness
**Status: GREEN**
`infrastructure` updated 11h ago. All others (chorus, music, photos, seeds) updated 3 days ago — within 7-day threshold. Resolved from RED on Jun 10.
**Action:** None. Monitor chorus domain (highest card velocity).

## 8. Disk Delta
**Status: YELLOW**
Repo at 345MB. No perf-baseline snapshot exists to diff against; scripts present but never run.
**Action:** Run `perf-baseline.sh` on host to capture first snapshot; enable nightly plist.
