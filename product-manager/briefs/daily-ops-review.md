# Daily Ops Review — 2026-06-19

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes; 8 dead-code warnings unchanged since 2026-06-02 (17-day carry). Dead: `has_test_run`/`has_production_code_edit` (tdd_gate.rs:169/197), `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64).
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; escalate to RED at next weekly.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
14+ plists route stdout/stderr to `/tmp/` (alert-notifier, chorus-api, chorus-hooks, context-cache-{daily,hourly,weekly}, fuseki-compact/perf, chorus-bridge, cruft-scan, etc.). Logs lost on reboot; count unchanged from Jun 16.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; 17-day carry with no movement.

## 3. CLAUDE.md Fragments
**Status: YELLOW**
21 shared fragments in `designing/claudemd/shared/` last touched Jun 5 (initial clone). Role CLAUDE.md pipeline stale since Feb 2026; no regenerated output in `roles/*/`.
**Action:** Wren — run claudemd pipeline to regenerate role CLAUDE.md files.

## 4. CSC Compliance
**Status: GREEN**
`messages/scripts/` and `architect/scripts/` absent (spec paths N/A). `platform/scripts/` and `proving/scripts/` — zero `/tmp/` violations found.
**Action:** None.

## 5. Git Dirty State
**Status: GREEN**
Working tree clean on main. Latest commit: #3489 (silas) 2026-06-18 20:07 UTC-4. 8 cards shipped Jun 18. Repos jeff-bridwell-personal-site, shared-observability, wordpress-blog not reachable from this clone.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED**
Board snapshots are **73 days stale** (last refreshed 2026-04-07). True WIP state unknown; 17-day carry. 8 cards shipped Jun 18 with no board update visible from this env.
**Action:** Wren/Silas — refresh board snapshot; wire daily LaunchAgent capture. Blocked without host access.

## 7. Domain Context Freshness
**Status: RED**
`domain-context-chorus.md` now 14 days stale (last updated ~Jun 5) while chorus/owl-api cards continue shipping daily (#3489, #3488, #3485, #3481, #3478 all Jun 18). Music/photos/seeds also ~14 days stale. Infrastructure current (Jun 18, 1 day).
**Action:** Wren — `domain-context-chorus.md` is highest priority (14-day drift + active shipping); music/photos/seeds need owner assignment.

## 8. Disk Delta / Perf Baseline
**Status: YELLOW**
No perf-baseline time-series available in this environment. `com.chorus.perf-baseline.plist` exists but logs to `/tmp/`; no captured output. Delta cannot be computed; 17-day carry.
**Action:** Run `platform/scripts/perf-baseline.sh` on host to establish baseline.
