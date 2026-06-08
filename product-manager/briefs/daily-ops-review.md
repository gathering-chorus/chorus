# Daily Ops Review — 2026-06-08

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (0 errors, 8 warnings) at `platform/services/chorus-hooks`. Dead-code warnings persist unchanged: `load_role_sections` never called (`protocol_contract.rs:155`), `chorus_worktree_override` never read (`types.rs:55`).
**Action:** Silas to resolve dead-code warnings or add `#[allow(dead_code)]` with comment if intentionally retained.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
18 plists in `proving/config/launchagents/` and `platform/services/chorus-hooks/` use `/tmp` for `StandardOutPath`/`StandardErrorPath`. Log data lost on reboot.
**Action:** Low urgency; migrate log paths to `~/Library/Logs/Chorus/` for post-reboot diagnosis. `com.chorus.tmp-reaper.plist` already uses the right pattern.

## 3. CLAUDE.md Fragments
**Status: GREEN**
`messages/claudemd/` directory absent; single `CLAUDE.md` at repo root only. No fragmentation to diff.
**Action:** None.

## 4. CSC Compliance
**Status: GREEN**
No `/tmp/` references in `messages/scripts/` or `architect/scripts/` (those dirs live in separate repos as expected).
**Action:** None.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — no uncommitted changes. Last 3 commits are today/yesterday wren briefs and daily reviews.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW**
Board lives in Vikunja (not queryable from this worktree). Latest commit activity shows no role work since Jun 6; most recent card merges were #3279 (wren), #3277 (silas), #3270 (kade) all merged within the past 48h.
**Action:** Wren to verify Vikunja board directly for any WIP cards open >48h at session start.

## 7. Domain Context Freshness
**Status: GREEN**
All 5 domain-context files (chorus, infrastructure, music, photos, seeds) last updated Jun 5 — 3 days ago, within the 7-day threshold. 55 commits landed in the past 7 days; only one domain-context update, but age is still within policy.
**Action:** Watch — chorus and photos domains most active; next update due by Jun 12.

## 8. Disk Delta
**Status: YELLOW**
No perf-baseline snapshot on this host; `perf-baseline.sh` and `perf-baseline-chorus.sh` scripts exist but no captured output to diff.
**Action:** Run `perf-baseline.sh` on host to establish baseline; schedule daily capture via `com.chorus.perf-baseline.plist`.
