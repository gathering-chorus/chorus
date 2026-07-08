# Daily Ops Review — 2026-07-08

## 1. Hooks Health
**Status: YELLOW (carry, 34d)**
`cargo check` passes with 8 warnings — count unchanged since Jun 4. Dead code: `load_role_sections`, `find_most_recent_pending`, `handle_approval_request`, `is_demo_or_done`, `has_test_run`, `has_production_code_edit`, `at_step` (×3), `chorus_worktree_override`. No errors.
**Action:** Silas — suppress or delete; 34-day carry with no movement, assign to next triage.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17 distinct plist files in `proving/config/launchagents/` use `/tmp/` for stdout/stderr (hooks, api, clearing, context-cache, fuseki, ops, etc.). Structural, unchanged.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; no card assigned yet.

## 3. CLAUDE.md Fragments
**Status: GREEN (resolved)**
All 24 shared fragments in `designing/claudemd/shared/` show 2026-07-08 today — fully refreshed. Role-specific fragments (wren/silas/kade) also current.
**Action:** None.

## 4. CSC Compliance
**Status: RED (carry)**
52 non-test platform scripts have hardcoded `/tmp/` paths. `messages/scripts/` and `architect/scripts/` paths from spec don't exist — checked `platform/scripts/` as the live equivalent. Core violations: `look.sh`, `bridge-subscriber-watchdog.sh`, `werk-init.sh`, `bedroom-heartbeat.sh`, `index-crawler-snapshots.sh`.
**Action:** Silas — `bridge-subscriber-watchdog.sh` /tmp state dir is highest risk; assign July card.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes across all role directories.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 104d)**
WF-165 (card #1704 — "Fix session-start board unreachable") created 2026-03-26, never started, still `in_progress`. Step 1 (Kade) shows `status: ready` with no `started_at`. 104 days untouched.
**Action:** Wren — close or re-groom #1704; it's planning debt at 100+ days.

## 7. Domain Context Freshness
**Status: GREEN (resolved)**
All 5 domain-context files (chorus, infrastructure, music, photos, seeds) updated today 2026-07-08. Drift from previous review cleared.
**Action:** None.

## 8. Disk Delta
**Status: N/A**
No runtime perf-baseline snapshots committed to repo (`perf-baseline.sh` writes to host only). Cannot compute delta from remote context.
**Action:** Silas — surface nightly baseline JSON to a repo path for cross-session delta tracking.
