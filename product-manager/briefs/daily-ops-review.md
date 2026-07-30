# Daily Ops Review — 2026-07-30

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes; 2 dead-code warnings unchanged: `registration_json` (session_registry.rs:66), `owes_response_block` (nudge_drain.rs:178). No regression, no fix.
**Action:** Silas — suppress or remove both dead-code paths; open if no card exists.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
33 `/tmp/` refs across 17 plist files in `proving/config/launchagents/`. Count unchanged.
**Action:** Silas — migration card open; no movement.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry)**
All fragments last committed 2026-07-24 (6d). Manifest build 217; changelog last 2026-04-17.
**Action:** Wren + Kade — role content refresh overdue; crossing 7d tomorrow.

## 4. CSC Compliance
**Status: RED (carry)**
`messages/scripts/` and `architect/scripts/` do not exist. `platform/scripts/` has **149 `/tmp/`** occurrences across 67 files — up 7 from Jul 29 (142).
**Action:** Silas — new occurrences added; remediation card needed; update prompt check-paths to `platform/scripts/`.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes across all tracked role directories.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW (carry)**
2 Dependabot PRs in chorus repo at **57d open**: #449 (`@cucumber/cucumber` 12→13), #443 (`ureq` 2→3, auto-rebase disabled). Out of scope for gathering-team MCP; status unverifiable here.
**Action:** Jeff/Silas — review and close or merge; #443 needs rebase re-enabled.

## 7. Domain Context Freshness
**Status: RED (escalating → expires tomorrow)**
All 5 domain-context files last committed 2026-07-24 — now **6d old**, threshold 7d. Files expire **2026-07-31**. Chorus context ~100d stale; 2 Silas cards shipped Jul 29 with no context update.
**Action:** Silas — update `domain-context-chorus.md` today before threshold breach.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` in `proving/logs/`; script targets macOS `diskutil`, not runnable in this env.
**Action:** Silas — land nightly baseline JSON to enable delta tracking.

---
*Jul 30 delta: §4 /tmp count up to 149 (+7 from 142 Jul 29). §7 escalating — all context files expire tomorrow (Jul 31). §3 fragments at 6d, cross 7d tomorrow. All other items carry unchanged.*
