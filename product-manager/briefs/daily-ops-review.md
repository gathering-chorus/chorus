# Daily Ops Review — 2026-07-01

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes (30s) with 8 warnings — count unchanged. Dead code: `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64), plus 6 others. No errors, no regression.
**Action:** Silas — suppress or delete; 27-day carry, no movement.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17 plist files in `proving/config/launchagents/` use `/tmp/` for stdout/stderr. Structural, unchanged.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; no card assigned yet.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry)**
50 fragments in `designing/claudemd/`; last committed 2026-06-21 (10 days). `chorus-prompt.md` and shared fragments lag behind #3581–#3598 shipped this week. Day count ticking up.
**Action:** Wren — refresh `chorus-prompt.md` and fragments referencing chorus protocol state.

## 4. CSC Compliance
**Status: RED (carry)**
58 non-test scripts in `platform/scripts/` have hardcoded `/tmp/` paths. Core violations unchanged: `bridge-subscriber.js` (runtime inbox), `coherence-check` (STATE_DIR), `look.sh`, `werk-init.sh`, `bedroom-heartbeat.sh`.
**Action:** Silas — `bridge-subscriber.js` runtime /tmp inbox is highest risk; assign card for July.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. Last commit: #3598 (kade). No action.

## 6. Stale WIP Cards
**Status: RED (carry)**
WF-165 (card #1704) in_progress since 2026-03-26 (97 days), 0 steps completed. #1759 [Wren] + #1791 [Silas] now 84 days in WIP with no repo activity.
**Action:** Wren + Silas — close, park, or re-groom today; three 80+ day WIPs is planning debt.

## 7. Domain Context Freshness
**Status: RED (escalated)**
All 5 domain-context files last committed 2026-06-21 (10 days). Card #3598 shipped today; 6 chorus cards (#3581–#3598) shipped since last update with zero context refresh. 10-day drift crossed the 7-day threshold.
**Action:** Wren — `domain-context-chorus.md` most urgent; flag if other domains gained cards too.

## 8. Disk Delta
**Status: N/A**
No runtime perf-baseline snapshots in repo (`perf-baseline.sh` outputs to host only). Cannot compute delta from remote context.
**Action:** Silas — surface nightly baseline JSON to a repo path for cross-session delta tracking.
