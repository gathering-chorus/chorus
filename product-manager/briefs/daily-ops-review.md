# Daily Ops Review — 2026-06-29

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes (36s) with 8 warnings — count unchanged from yesterday. Dead code: `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64), plus 6 others. No regression, no remediation.
**Action:** Silas — suppress or delete; 25-day carry, no movement.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
36 plist files use `/tmp/` for stdout/stderr across `proving/config/launchagents/`, `config/launchagents/`, and `platform/scripts/launchagents-canonical/`. Count unchanged from yesterday. Structural, not incidental.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; file a card to scope the sweep.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry)**
`designing/claudemd/shared/` has 21 fragments; 18/21 last touched Jun 5 (24 days). 3 updated today (cross-machine-operations-core/reference, portfolio-reference). Chorus domain shipping heavily (#3581–#3596) but `chorus-prompt.md` and most shared fragments lag.
**Action:** Wren — refresh `chorus-prompt.md` and any shared fragments that reference chorus protocol state.

## 4. CSC Compliance
**Status: RED (carry)**
15 non-plist scripts in `platform/scripts/` have hardcoded `/tmp/` paths: `bridge-subscriber.js` (runtime inbox), `coherence-check`, `look.sh`, `bedroom-heartbeat.sh`, `index-crawler-snapshots.sh`, `werk-init.sh`, others. Unresolved since May review.
**Action:** Silas — assign owner or timebox for July. `bridge-subscriber.js` is highest risk (runtime, role-scoped inbox path).

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. Last commits: #3593 (silas) and #3596 (kade) merged via PRs #713/#712. No action.

## 6. Stale WIP Cards
**Status: RED**
Board snapshot shows exactly 2 WIP cards, both untouched since 2026-04-07 (82 days): #1759 [Wren] "Framework service design — OWL entity model" and #1791 [Silas] "Restore chorus product boundary". No fresh WIP.
**Action:** Wren + Silas — close, park, or re-groom both cards today; 82-day WIP limbo is a planning liability.

## 7. Domain Context Freshness
**Status: YELLOW**
`domain-context-infrastructure.md` updated today (Jun 29) — fresh. `domain-context-chorus.md` last touched Jun 5 (24 days); 5+ cards shipped in chorus domain this week (#3581, #3587, #3590, #3593, #3596). Music/photos/seeds stale at 24 days but no active cards in those domains.
**Action:** Wren or Silas — update `domain-context-chorus.md`; chorus context has drifted past shipped state.

## 8. Disk Delta
**Status: N/A**
No runtime perf-baseline snapshots committed to repo (`perf-baseline.sh` exists but outputs to host only). Cannot compute delta from remote context.
**Action:** Silas — surface nightly baseline JSON to repo path for cross-session delta tracking.
