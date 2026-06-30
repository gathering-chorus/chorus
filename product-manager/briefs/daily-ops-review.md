# Daily Ops Review — 2026-06-30

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes (29s) with 8 warnings — count unchanged. Dead code: `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64), plus 6 others. No errors, no regression.
**Action:** Silas — suppress or delete; 26-day carry, no movement.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
36 plist files use `/tmp/` for stdout/stderr across `proving/config/launchagents/` and platform. Count unchanged from yesterday. Structural, not incidental.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; no card assigned yet.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry)**
50 fragments in `designing/claudemd/`; last committed 2026-06-21 (9 days). No fragment updated since chorus cards #3581–#3596 shipped this week. `chorus-prompt.md` and shared fragments lag ship state.
**Action:** Wren — refresh `chorus-prompt.md` and any fragments that reference chorus protocol state.

## 4. CSC Compliance
**Status: RED (carry)**
37 scripts in `platform/scripts/` have hardcoded `/tmp/` paths (yesterday's 15 was conservative; today's broader grep includes test scripts). Core violations unchanged: `bridge-subscriber.js` (runtime inbox), `look.sh`, `werk-init.sh`, `bedroom-heartbeat.sh`.
**Action:** Silas — assign owner or timebox for July; `bridge-subscriber.js` remains highest risk.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. Last commit: silas daily-ops-review 2026-06-29. No action.

## 6. Stale WIP Cards
**Status: RED (carry)**
#1759 [Wren] "Framework service design — OWL entity model" and #1791 [Silas] "Restore chorus product boundary" — now 83 days in WIP, no activity.
**Action:** Wren + Silas — close, park, or re-groom today; 83-day WIP limbo is a planning liability.

## 7. Domain Context Freshness
**Status: RED (escalated)**
`domain-context-chorus.md` content last updated 2026-04-19 (72 days ago by Silas). Jun 21 commit was a structural move, not a content refresh. 6 chorus cards shipped this week (#3581–#3596) with zero context update. Music/photos/seeds last committed Jun 21, no active cards.
**Action:** Wren — update `domain-context-chorus.md`; 72-day drift with active shipping is a real hazard.

## 8. Disk Delta
**Status: N/A**
No runtime perf-baseline snapshots committed to repo (`perf-baseline.sh` outputs to host only). Cannot compute delta from remote context.
**Action:** Silas — surface nightly baseline JSON to repo path for cross-session delta tracking.
