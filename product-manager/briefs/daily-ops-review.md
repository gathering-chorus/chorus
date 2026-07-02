# Daily Ops Review — 2026-07-02

## 1. Hooks Health
**Status: YELLOW (carry, 28d)**
`cargo check` passes with 8 warnings — count unchanged. Dead code: `load_role_sections`, `find_most_recent_pending`, `handle_approval_request`, `is_demo_or_done`, `has_test_run`, `has_production_code_edit`, `at_step` (×3), `chorus_worktree_override`. No errors.
**Action:** Silas — suppress or delete; 28-day carry with no movement.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17+ plist files in `proving/config/launchagents/` use `/tmp/` for stdout/stderr. Structural, unchanged.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; no card assigned yet.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry, 11d)**
50 fragments in `designing/claudemd/`; last committed 2026-06-22 (11 days). `chorus-prompt.md` and shared fragments lag #3581–#3602 shipped this week. Day count ticking up.
**Action:** Wren — refresh fragments referencing chorus protocol state; worktree-convention.md may need #2913 update.

## 4. CSC Compliance
**Status: RED (carry)**
58 non-test scripts in `platform/scripts/` have hardcoded `/tmp/` paths (requested `messages/scripts/` + `architect/scripts/` don't exist). Core violations: `bridge-subscriber.js`, `coherence-check`, `look.sh`, `werk-init.sh`, `bedroom-heartbeat.sh`.
**Action:** Silas — `bridge-subscriber.js` runtime /tmp inbox is highest risk; assign July card.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. Last commit: `c76068b` silas: daily quality review 2026-07-02. No action.

## 6. Stale WIP Cards
**Status: RED (carry, unverifiable)**
Board CLI unavailable in remote env; snapshots absent. Carry from Jul 1: #1704 (98+ days), #1759/#1791 (85+ days WIP, no repo activity).
**Action:** Wren + Silas — close, park, or re-groom; three 80+ day WIPs is planning debt.

## 7. Domain Context Freshness
**Status: RED (carry, 11d)**
All 5 domain-context files last committed 2026-06-22 (11 days). Cards #3589/#3590/#3594/#3596/#3598/#3602 shipped since last update; chorus domain most impacted. 11-day drift on active domains.
**Action:** Wren — `domain-context-chorus.md` most urgent; refresh today.

## 8. Disk Delta
**Status: N/A**
No runtime perf-baseline snapshots in repo (`perf-baseline.sh` outputs to host only). Cannot compute delta from remote context.
**Action:** Silas — surface nightly baseline JSON to a repo path for cross-session delta tracking.
