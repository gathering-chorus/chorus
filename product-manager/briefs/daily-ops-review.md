# Daily Ops Review — 2026-06-26

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes (30s clean); same 8 dead-code warnings, now 24-day carry. Confirmed today: `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64) still present.
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete dead code; no movement in 24 days.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17+ plists in `proving/config/launchagents/` use `/tmp/` for StandardOut/Err (hooks, api, context-cache ×3, alert-notifier, clearing, fuseki ×2, ops, cruft-scan, nudge-health, perf-baseline, harvest-exporter, jeff-input-monitor). 24-day carry.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; blocked without host LaunchAgent access.

## 3. CLAUDE.md Fragments
**Status: YELLOW (approaching threshold)**
`designing/claudemd/` (roles/ + shared/, 24+ fragments) last committed Jun 21 — 5 days, threshold Jun 28. Chorus domain shipping hard (#3593, #3596, #3581 this week); fragments lag. No role CLAUDE.md files exist in `roles/` subdirs.
**Action:** Wren — run claudemd pipeline before Jun 28; focus on chorus-prompt.md and shared protocol fragments.

## 4. CSC Compliance
**Status: YELLOW (carry)**
36 scripts in `platform/scripts/` reference `/tmp/`; ~15 non-test uses hardcoded (look.sh, bedroom-heartbeat.sh, deep-health.sh, chorus-query.sh, crawler-hydrate-graph.sh). DEC-114 is detected at runtime by deep-health.sh but not remediated at source.
**Action:** Silas — annotate legitimate /tmp uses; migrate state files to `$CHORUS_HOME` paths.

## 5. Git Dirty State
**Status: GREEN**
Working tree clean across all tracked dirs. Today: #3593 (silas) + #3596 (kade) merged. No uncommitted changes. No action.

## 6. Stale WIP Cards
**Status: UNKNOWN (carry)**
Live Vikunja board not queryable from remote container; no snapshot committed to repo. Prior reading (Jun 25) flagged 2 cards untouched since Apr 7 (now 80 days): "Framework service design — OWL entity model" and "Restore chorus product boundary".
**Action:** Jeff or Wren — pull board snapshot; confirm stale cards are closed, blocked, or re-groomed.

## 7. Domain Context Freshness
**Status: YELLOW (approaching threshold)**
`domain-context-chorus.md` last committed Jun 21 (5 days); 10+ Chorus cards shipped since (#3593, #3596, #3581, #3586, #3584, #3582, #3573, #3579, #3575). Will breach 7-day threshold Jun 28. `domain-context-infrastructure.md` fresh (Jun 26). Music/photos/seeds unchanged but no active cards in those domains.
**Action:** Wren — update domain-context-chorus.md today or tomorrow; infrastructure OK.

## 8. Disk Delta
**Status: N/A**
No perf-baseline snapshots in repo (LaunchAgent outputs to host only; `proving/logs/perf-baseline-*.json` not committed). Cannot compute delta from remote context.
**Action:** Silas — surface nightly perf-baseline JSON to repo for delta tracking across sessions.
