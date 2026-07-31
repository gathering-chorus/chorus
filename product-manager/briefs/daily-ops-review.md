# Daily Ops Review — 2026-07-31

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` not runnable in remote env (no root Cargo.toml). Previous state carries: 2 dead-code warnings (`registration_json` session_registry.rs:66, `owes_response_block` nudge_drain.rs:178). No regression reported.
**Action:** Silas — suppress or remove both dead-code paths; verify next local run.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
33 `/tmp/` refs across 17 plist files in `proving/config/launchagents/`. Count unchanged from Jul 30.
**Action:** Silas — migration card open; no movement.

## 3. CLAUDE.md Fragments
**Status: RED (breached today)**
`messages/claudemd/` does not exist — prompt path wrong; actual path `designing/claudemd/`. All fragments last committed 2026-07-25 via #3356; Wren morning summary declares 7d threshold breached today.
**Action:** Wren + Kade — refresh role content now; update ops prompt path to `designing/claudemd/`.

## 4. CSC Compliance
**Status: RED (carry)**
`messages/scripts/` and `architect/scripts/` do not exist. `platform/scripts/` has **149 `/tmp/`** occurrences — unchanged from Jul 30. Prompt check-paths still incorrect.
**Action:** Silas — confirm count held; no new regressions. Update prompt to scan `platform/scripts/`.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes. Other 6 role dirs not present in this clone; `product-manager` is clean.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW (carry)**
2 Dependabot PRs now at **58d open**: #449 (`@cucumber/cucumber` 12→13), #443 (`ureq` 2→3, auto-rebase disabled). Board CLI not accessible from remote env.
**Action:** Jeff/Silas — review and close or merge; #443 needs rebase re-enabled.

## 7. Domain Context Freshness
**Status: RED (breached)**
All 5 domain-context files last committed 2026-07-25; Wren morning summary declares expiry **today**. Chorus context ~100d stale; 4 Silas/Kade cards shipped Jul 30 with no context update.
**Action:** Silas — update `domain-context-chorus.md` immediately (overdue).

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` in `proving/logs/`; script targets macOS `diskutil`, not runnable in remote env.
**Action:** Silas — land nightly baseline JSON to enable delta tracking.

---
*Jul 31 delta: §3 fragments and §7 domain context both breached 7d threshold today. §4 /tmp count held at 149 (no new regressions). §6 Dependabot at 58d (+1). Prompt paths for §3 and §4 need correction (see actions). All other items carry unchanged.*
