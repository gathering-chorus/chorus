# Daily Ops Review — 2026-06-06

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (0 errors, 8 warnings) at `platform/services/chorus-hooks`. Dead-code warnings unchanged from 2026-06-03: `load_role_sections` never called (`protocol_contract.rs:155`), `chorus_worktree_override` never read (`types.rs:55`).
**Action:** Silas to resolve dead-code warnings or document why retained.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
20+ plists use `/tmp` for `StandardOutPath`/`StandardErrorPath` (hooks, context-cache, harvest-exporter, alert-notifier, etc.). Log data lost on reboot; no persistent log dir.
**Action:** Evaluate migrating log paths to `~/Library/Logs/Chorus/` — already used by tmp-reaper. Low urgency but affects post-reboot diagnosis.

## 3. CLAUDE.md Fragments
**Status: GREEN**
`messages/claudemd/` directory does not exist; only one `CLAUDE.md` at repo root. No fragmentation to diff.
**Action:** None.

## 4. CSC Compliance
**Status: GREEN**
No `/tmp/` references in `messages/scripts/` or `architect/scripts/` (directories absent from this repo — expected for separate-repo roles).
**Action:** None.

## 5. Git Dirty State
**Status: YELLOW**
`product-manager/`: clean (0 dirty files). Remaining 6 role dirs (architect, engineer, messages, jeff-bridwell-personal-site, shared-observability, wordpress-blog) are not present in this worktree — live in separate repos or per-card worktrees.
**Action:** Spot-check canonical worktrees on host if full dirty-state audit needed.

## 6. Stale WIP Cards
**Status: GREEN**
No WIP-labeled open issues on GitHub. No open issues at all on gathering-team repo.
**Action:** None.

## 7. Domain Context Freshness
**Status: GREEN**
All 5 domain-context files (chorus, infrastructure, music, photos, seeds) updated within 1 day. No staleness.
**Action:** None.

## 8. Disk Delta
**Status: YELLOW**
Repo total: 343M. No prior perf-baseline snapshot available to diff; scripts (`perf-baseline.sh`, `perf-baseline-chorus.sh`) exist but no captured output on this host.
**Action:** Run `perf-baseline.sh` to establish a baseline for future comparisons.
