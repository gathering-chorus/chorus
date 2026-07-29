# Daily Ops Review — 2026-07-29

## 1. Hooks Health
**Status: YELLOW (regression)**
`cargo check` passes with **2** dead-code warnings: `registration_json` (session_registry.rs:66) and `owes_response_block` (nudge_drain.rs:178). Jul 28 brief said `registration_json` resolved — it is back in the Jul 24 snapshot tree.
**Action:** Silas — verify whether fix landed after Jul 24; suppress or remove both dead-code paths.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
Plists in `proving/config/launchagents/` and `platform/scripts/launchagents-*` reference `/tmp/` for stdout/stderr. Count unchanged.
**Action:** Silas — migration card open; no movement.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry)**
Shared fragments in `designing/claudemd/shared/` last committed 2026-07-24 (5d). Manifest at build 217, last changelog 2026-04-17. Fragment sync lag unresolved.
**Action:** Wren + Kade — role fragments need content refresh.

## 4. CSC Compliance
**Status: RED (carry)**
Check-paths `messages/scripts/` and `architect/scripts/` do not exist. `platform/scripts/` has **142** `/tmp/` occurrences across operational scripts — up from Jul 28 estimate of 37 files.
**Action:** Silas — remediation card needed; update prompt check-paths to `platform/scripts/`.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes across all tracked role directories.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW**
No open GitHub issues. 2 aged Dependabot PRs (chorus repo, 56d open): **#449** (`@cucumber/cucumber` 12→13) and **#443** (`ureq` 2→3), both major breaking; #443 auto-rebase disabled.
**Action:** Jeff/Silas — review and merge or close; #443 needs rebase re-enabled.

## 7. Domain Context Freshness
**Status: RED (carry)**
All 5 domain-context files last committed 2026-07-24 (5d, threshold 7d). Chorus content ~100d stale per Jul 28; 4 chorus/infra cards shipped Jul 27 with no context update.
**Action:** Silas — chorus context critical; all 5 files expire Jul 31.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` in `proving/logs/`. Script targets macOS `diskutil`; not runnable in this env.
**Action:** Silas — land nightly baseline JSON to `proving/logs/` to enable delta tracking.

---
*Jul 29 delta: §1 regression (1→2 warnings, `registration_json` returned). §4 count corrected to 142 occurrences. §6 switched to GitHub PRs (no open issues found). Watch: domain context hits 7d threshold Jul 31.*
