# Daily Ops Review — 2026-06-24

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (37s clean build); same 8 dead-code warnings, now 22-day carry. Dead: `has_test_run` + `has_production_code_edit` (tdd_gate.rs), `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64), plus 4 others.
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; no change since Jun 2, escalate to RED at next weekly.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
30+ plists in `proving/config/launchagents/` use `/tmp/` for StandardOut/Err (hooks, api, context-cache ×4, alert-notifier, clearing, fuseki ×2, harvest-exporter, cruft-scan, nudge-health, ops, etc.). 22-day carry.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; blocked without host LaunchAgent access.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry)**
`designing/claudemd/shared/` (24 fragments) last committed Jun 20 — 4-day lag. `messages/claudemd/` not accessible; prior finding of 18-day carry on role files unconfirmed. Heavy shipping Jun 21–23 (board domain, #3545/#3551/#3573/#3575) not yet reflected.
**Action:** Wren — run claudemd pipeline post-shipping sprint; confirm role CLAUDE.md regenerated.

## 4. CSC Compliance
**Status: YELLOW**
`messages/scripts/` and `architect/scripts/` absent in this repo. `platform/scripts/` has 15+ non-test scripts with `/tmp/` refs (coherence-check, look.sh, werk-init.sh, bedroom-heartbeat.sh, bridge-subscriber-watchdog.sh, index-crawler-snapshots.sh, etc.). Not all are CSC violations (some use /tmp intentionally), but no audit trail.
**Action:** Silas — audit platform/scripts/ /tmp/ usages against CSC convention; annotate intentional vs. non-compliant.

## 5. Git Dirty State
**Status: GREEN**
Working tree clean. Latest commit: #3573 (silas, Jun 23). Roles active: silas Jun 23 (#3573, #3579), wren Jun 23 (#3575, #3432), kade Jun 23 (#2819). No action.

## 6. Stale WIP Cards
**Status: RED (carry)**
No board snapshot accessible in this environment. Prior reading (Jun 22): 2 cards untouched since Apr 7 (78 days) — "Framework service design — OWL entity model" and "Restore chorus product boundary". No update received.
**Action:** Jeff or Wren — refresh board snapshot; confirm whether stale cards are closed or blocked.

## 7. Domain Context Freshness
**Status: RED**
`domain-context-chorus.md` last updated Apr 19 (66 days); `domain-context-infrastructure.md` last updated Mar 25 (91 days). Both domains had heavy shipping this week (chorus: #3545/#3551/#3573/#3575; infra: #3560 CMDB/fuseki-backup Jun 22) — 66–91 days of drift well past the 7-day threshold.
**Action:** Wren — update domain-context-chorus.md and domain-context-infrastructure.md immediately; both are RED overdue.

## 8. Disk Delta
**Status: UNKNOWN**
No perf-baseline JSON in repo (`data/athena/tree.json` present but no size snapshots). `com.chorus.perf-baseline` LaunchAgent defined but outputs to host only.
**Action:** Silas — surface perf-baseline snapshot to `proving/logs/perf-baseline-latest.json` for remote visibility.
