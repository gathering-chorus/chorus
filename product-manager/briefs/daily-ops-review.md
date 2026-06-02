# Daily Ops Review — 2026-06-02

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (0 errors) but emits 8 warnings — dead-code noise persists: unused field `chorus_worktree_override` in `HookInput`, plus prior dead-code items unchanged from 2026-05-30 review.
**Action:** Dead-code cleanup card still open; warnings reduced from 15 → 8, trending right but not resolved.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
3 production plists write logs to `/tmp/`: `com.chorus.hooks.plist` (stdout+stderr), `com.chorus.chorus-ops.plist` (ops log). Purged on macOS reboot; logs lost across restarts.
**Action:** Known gap — migrate to `~/Library/Logs/Chorus/`; unchanged since last review.

## 3. CLAUDE.md Conflicts
**Status: GREEN**
No `messages/claudemd/` fragment directory. Single `CLAUDE.md` at repo root, last committed 2026-05-30 (3 days ago).
**Action:** None.

## 4. CSC Compliance (/tmp in scripts)
**Status: GREEN**
No `/tmp/` references in `messages/scripts/` or `architect/scripts/` (neither directory present in this clone).
**Action:** None.

## 5. Git Dirty State
**Status: GREEN**
All 7 role directories clean (0 uncommitted changes).
**Action:** None.

## 6. Stale WIP Cards
**Status: RED**
Board snapshots (wren/silas/kade) are 56 days old — last captured 2026-04-07. Snapshot shows 0 WIP cards but data is too stale to trust. No mechanism currently re-captures snapshots automatically.
**Action:** Run board snapshot capture now; wire `chorus-ops` or a LaunchAgent to refresh daily.

## 7. Domain Context Freshness
**Status: GREEN**
All 5 `domain-context-*.md` files are 3 days old (chorus, infrastructure, music, photos, seeds) — well within 7-day threshold.
**Action:** None; re-check after board snapshot refresh to verify against shipped cards.

## 8. Disk Delta / Perf Baseline
**Status: GREY**
`perf-baseline.sh` and `perf-baseline-chorus.sh` exist; no captured baseline output in repo or platform/logs/. Cannot compute delta.
**Action:** Run `platform/scripts/perf-baseline.sh` and commit output to establish baseline.
