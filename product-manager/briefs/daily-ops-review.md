# Daily Ops Review — 2026-06-23

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (28s clean build); 8 dead-code warnings, now 21-day carry (unchanged since Jun 2). Dead: `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64), plus 6 others.
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; escalate to RED at next weekly.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
17+ plists across `proving/config/launchagents/` and `platform/services/` still use `/tmp/` for StandardOut/Err paths (hooks, api, context-cache ×3, alert-notifier, clearing, fuseki, harvest-exporter, etc.). 21-day carry.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; blocked without host LaunchAgent access.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry)**
`messages/claudemd/` not accessible in this remote environment; cannot diff fragments. Previous finding (Jun 22): 45 of 48 fragments at Jun 5 mtime (18-day carry), 3 shared fragments updated Jun 22. No confirmed regeneration of role files.
**Action:** Wren — run claudemd pipeline; confirm role CLAUDE.md files were regenerated from Jun 22 fragment updates.

## 4. CSC Compliance
**Status: GREEN**
No `/tmp/` refs found in `messages/scripts/` or `architect/scripts/`. Clean.

## 5. Git Dirty State
**Status: GREEN**
All 7 role directories clean. Latest commit: #3560 (silas, Jun 21, fuseki-backup + CMDB schema). No action.

## 6. Stale WIP Cards
**Status: RED**
Board snapshots not present in this environment (`platform/logs/board-snapshot-*.json` absent). Prior reading (Jun 22): snapshots 76 days old (Apr 7), 2 WIP cards untouched since Apr 7 — "Framework service design — OWL entity model" and "Restore chorus product boundary". Now 77-day carry.
**Action:** Jeff or Wren — refresh board snapshot urgently; confirm whether cards closed or truly stalled.

## 7. Domain Context Freshness
**Status: YELLOW**
All 5 domain-context files last touched Jun 19 (4 days). Chorus and infrastructure domains had commits Jun 21 (#3560 silas: fuseki-backup + CMDB; #3569 silas: nightly-coverage) not reflected in their contexts. Under 7-day threshold but drifting after active shipping week (50 commits in 7 days).
**Action:** Wren — update domain-context-chorus.md and domain-context-infrastructure.md for Jun 21 infra work before next 7-day trigger.

## 8. Disk Delta
**Status: UNKNOWN**
No perf-baseline JSON logs available in this environment (repo is 349 MB total). `com.chorus.perf-baseline` LaunchAgent defined but outputs to host only.
**Action:** Silas — surface latest perf-baseline snapshot to repo (e.g., `proving/logs/perf-baseline-latest.json`) for remote visibility.
