# Daily Ops Review — 2026-06-22

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes; 8 dead-code warnings, 20-day carry (unchanged since Jun 2). Dead: `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64), plus 6 others.
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; escalate to RED at next weekly.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
17 plists in `proving/config/launchagents/` reference `/tmp/` for log paths (hooks, api, clearing, context-cache ×3, cruft-scan, fuseki-{compact,perf}, harvest-exporter, jeff-input-monitor, launchagent-metrics, nudge-health, ops, perf-baseline). 20-day carry.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; blocked without host LaunchAgent access.

## 3. CLAUDE.md Fragments
**Status: YELLOW**
3 shared fragments updated today (cross-machine-operations-{core,reference}.md, portfolio-reference.md); 45 of 48 fragments still at Jun 5 mtime (17-day carry). No role CLAUDE.md regeneration visible.
**Action:** Wren — run claudemd pipeline to regenerate role files from updated shared fragments.

## 4. CSC Compliance
**Status: YELLOW**
9+ `/tmp/` refs in `platform/scripts/`: coherence-check (PULSE_FILE, STATE_DIR), werk-init.sh (CACHE), bedroom-heartbeat.sh, bridge-subscriber-watchdog.sh, look.sh, bridge-subscriber.js, index-crawler-snapshots.sh, crawler-hydrate-graph.sh. 20-day carry.
**Action:** Silas — categorize transient state (acceptable) vs. persistent log paths (must migrate).

## 5. Git Dirty State
**Status: GREEN**
Clean. Latest commit: #3551 (wren, Jun 22). External repos not in this clone. No action.

## 6. Stale WIP Cards
**Status: RED**
Board snapshots 76 days old (Apr 7). Two WIP cards both stuck at Apr 7: "Framework service design — OWL entity model" and "Restore chorus product boundary". 20-day carry with no board refresh.
**Action:** Jeff or Wren — refresh board snapshot; confirm whether cards closed or truly stale.

## 7. Domain Context Freshness
**Status: YELLOW**
4 of 5 domain-context files at Jun 5 (17 days): chorus, music, photos, seeds. Chorus domain had 8+ commits this week (#3528–#3551) but context doc untouched. Infrastructure updated today.
**Action:** Wren — update domain-context-chorus.md to reflect CI work, worktree model, and binary deploy changes from the last two weeks.

## 8. Disk Delta
**Status: UNKNOWN**
No perf-baseline JSON logs in this environment (`proving/logs/perf-baseline-YYYY-MM-DD.json` on host only). Cannot compare growth.
**Action:** Verify `com.chorus.perf-baseline` LaunchAgent fires on host; surface logs to repo.
