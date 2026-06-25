# Daily Ops Review — 2026-06-25

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes (30s); same 8 dead-code warnings, now 23-day carry. Confirmed: `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64), plus 6 others (tdd_gate.rs, etc.).
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; zero movement since Jun 2.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17 confirmed plists in `proving/config/launchagents/` use `/tmp/` for StandardOut/Err (hooks, api, context-cache ×3, alert-notifier, clearing, fuseki ×2, ops, posture-capture, cruft-scan, nudge-health, perf-baseline, harvest-exporter, jeff-input-monitor). 23-day carry.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; blocked without host LaunchAgent access.

## 3. CLAUDE.md Fragments
**Status: YELLOW**
`designing/claudemd/shared/` (24 fragments) last committed Jun 20 — now 5-day lag. Heavy shipping Jun 21–24 (#3570, #3575, #3432, #3391, #3573, #3582) not yet reflected. No role CLAUDE.md files found in `roles/` subdirs (pipeline may not have run).
**Action:** Wren — run claudemd pipeline; fragments will breach 7-day threshold by Jun 27.

## 4. CSC Compliance
**Status: YELLOW (carry)**
`platform/scripts/` has 15+ non-test `/tmp/` refs: coherence-check (state dir), look.sh, werk-init.sh (×2), bedroom-heartbeat.sh, bridge-subscriber.js, bridge-subscriber-watchdog.sh, index-crawler-snapshots.sh, health-check-bedroom.sh. No CSC audit trail exists.
**Action:** Silas — annotate intentional /tmp usages vs. non-compliant; create audit issue.

## 5. Git Dirty State
**Status: GREEN**
Working tree clean. Latest commit #3582 (silas, Jun 24). Recent activity: silas #3573/#3579/#3582, wren #3432/#3575/#3391/#3570, kade #2819. No action.

## 6. Stale WIP Cards
**Status: RED (carry)**
Live Vikunja board not accessible in this environment; no snapshot in repo. Prior reading (Jun 22): 2 cards untouched since Apr 7 (now 79 days) — "Framework service design — OWL entity model" and "Restore chorus product boundary". No update for 3 days.
**Action:** Jeff or Wren — pull board snapshot; confirm stale cards are closed, blocked, or groomed.

## 7. Domain Context Freshness
**Status: YELLOW**
All 5 domain-context files last committed Jun 20 (5 days). Under 7-day threshold today, but Chorus domain has 6 cards shipped since then (#3570, #3575, #3432, #3391, #3573, #3582). Will breach threshold Jun 27.
**Action:** Wren — update domain-context-chorus.md pre-threshold; infrastructure also approaching (1 card since Jun 20: #3560 CMDB/fuseki-backup).

## 8. Disk Delta
**Status: UNKNOWN**
Repo is 649MB total. No perf-baseline snapshot accessible (LaunchAgent defined in plists, outputs to host only; no `proving/logs/perf-baseline-latest.json` in repo).
**Action:** Silas — surface latest perf-baseline run to repo for remote delta comparison.
