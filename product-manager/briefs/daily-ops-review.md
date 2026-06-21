# Daily Ops Review — 2026-06-21

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes; 8 dead-code warnings, 19-day carry (unchanged since Jun 2). Dead: `has_test_run`/`has_production_code_edit` (tdd_gate.rs:169/197), `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64).
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; escalate to RED at next weekly.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
25+ `/tmp/` log-path refs across 15+ plists in `proving/config/launchagents/` (hooks, api, clearing, context-cache x3, cruft-scan, fuseki-compact, fuseki-perf, harvest-exporter, jeff-input-monitor, launchagent-metrics, nudge-health, ops, perf-baseline). Violates wren/decisions.md log-routing decision. 19-day carry.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; blocked without host LaunchAgent access.

## 3. CLAUDE.md Fragments
**Status: YELLOW**
Spec path `messages/claudemd/` absent; fragments live in `designing/claudemd/shared/`. Three files updated today (cross-machine-operations-{core,reference}, portfolio-reference); remainder at Jun 5 mtime (16-day carry). No role CLAUDE.md regeneration visible.
**Action:** Wren — run claudemd pipeline to regenerate role files; correct spec path in ops checklist.

## 4. CSC Compliance
**Status: YELLOW** *(corrected from yesterday's GREEN)*
`platform/scripts/` has ~15 `/tmp/` refs: coherence-check (PULSE_FILE, STATE_DIR), werk-init.sh (CACHE), bedroom-heartbeat.sh, bridge-subscriber-watchdog.sh, look.sh, bridge-subscriber.js, index-crawler-snapshots.sh, crawler-hydrate-graph.sh, others. Spec paths (`messages/scripts/`, `architect/scripts/`) absent; platform/scripts/ scanned as equivalent.
**Action:** Silas — categorize: transient state (acceptable) vs. persistent log paths (must migrate to `~/Library/Logs/`).

## 5. Git Dirty State
**Status: GREEN**
Working tree clean. Latest commit: #3528 (silas, green-main CI batch) Jun 20. jeff-bridwell-personal-site, shared-observability, wordpress-blog not in this clone.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED**
Board snapshot internal timestamp: 2026-04-07 (75 days old). Two WIP cards: #1759 Wren (Framework service design) and #1791 Silas (Restore chorus product boundary) — both Apr 7, presumably resolved given cards now at #3528. Live board state unknown; 19-day carry.
**Action:** Wren/Silas — push fresh board snapshot; wire daily LaunchAgent capture. Blocked without host access.

## 7. Domain Context Freshness
**Status: RED** *(escalated from YELLOW — yesterday's #3433 reference was spurious; card not in git)*
Content-based last-updated dates: chorus 2026-04-19 (63 days), infrastructure 2026-03-25 (88 days), music/photos 2026-03-26 (87 days), seeds 2026-04-01 (81 days). All exceed 7-day threshold. Chorus cards shipping daily (#3499–#3528 past 7 days). Critical domain drift.
**Action:** Wren — update `domain-context-chorus.md` now (most active domain). Silas — infrastructure. All 5 files overdue.

## 8. Disk Delta / Perf Baseline
**Status: YELLOW**
No perf-baseline time-series available in this environment. `com.chorus.perf-baseline.plist` routes to `/tmp/perf-baseline-nightly.log` (volatile; lost on reboot). 19-day carry.
**Action:** Run `platform/scripts/perf-baseline.sh` on host; migrate plist log path to `~/Library/Logs/`.
