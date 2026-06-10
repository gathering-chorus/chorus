# Daily Ops Review — 2026-06-10

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (0 errors, 8 warnings) at `platform/services/chorus-hooks`. Same 8 dead-code warnings as Jun 8 — no regression, no improvement. `chorus_worktree_override` (`types.rs:55`) and `load_role_sections` (`protocol_contract.rs:155`) still flagged.
**Action:** Silas — resolve or annotate with `#[allow(dead_code)]`; 2-day carry.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
17 plists in `proving/config/launchagents/` use `/tmp` for `StandardOutPath`/`StandardErrorPath`. Log data lost on reboot. Count unchanged from Jun 8.
**Action:** Low urgency; migrate to `~/Library/Logs/Chorus/`. No change since last review.

## 3. CLAUDE.md Fragments
**Status: GREEN**
`messages/claudemd/` absent. Fragments live at `designing/claudemd/shared/` (20 files). No divergence detectable from this worktree.
**Action:** None.

## 4. CSC Compliance
**Status: GREEN**
No `/tmp/` refs in `messages/scripts/` or `architect/scripts/` (dirs absent from this repo as expected).
**Action:** None.

## 5. Git Dirty State
**Status: GREEN**
All 7 role dirs clean (product-manager, architect, engineer, messages, jeff-bridwell-personal-site, shared-observability, wordpress-blog). No uncommitted changes.
**Action:** None.

## 6. Stale WIP Cards
**Status: GREEN**
Active shipping June 9 across all three roles: wren (#3318, #3284), silas (#3315, #3310, #3313, #3309, #3308), kade (#3297, #3296, #3306). No gap >48h visible in commit log.
**Action:** Verify Vikunja board directly for any open WIP not yet committed.

## 7. Domain Context Freshness
**Status: RED**
Git commit date Jun 5 is within threshold, but **content** `Last updated` headers are March–April 2026 (77 days for infrastructure, 76 for music/photos, 70 for seeds, 52 for chorus). Cards shipped Jun 9 in all domains; context is effectively stale.
**Action:** Each role — audit and refresh domain-context file for owned domain before next card. Chorus domain highest priority (most active).

## 8. Disk Delta
**Status: YELLOW**
`perf-baseline.sh` and `perf-baseline-chorus.sh` exist but no captured snapshot to diff.
**Action:** Run `perf-baseline.sh` on host to establish baseline; wire `com.chorus.perf-baseline-nightly.plist` if not already running.
