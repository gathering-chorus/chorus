# Daily Ops Review — 2026-06-20

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes; 8 dead-code warnings, now 18-day carry (unchanged since Jun 2). Dead: `has_test_run`/`has_production_code_edit` (tdd_gate.rs:169/197), `load_role_sections` (protocol_contract.rs:155), `chorus_worktree_override` (types.rs:64).
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; escalate to RED at next weekly.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
`com.chorus.hooks.plist` (3 copies: root, proving/, platform/) routes stdout/stderr to `/tmp/`; logs lost on reboot. 18-day carry, no movement.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; blocked without host LaunchAgent access.

## 3. CLAUDE.md Fragments
**Status: YELLOW**
`messages/claudemd/` path absent (spec mismatch); fragments live in `designing/claudemd/shared/`, last touched Jun 5 (15-day carry). No regenerated output in `roles/*/`.
**Action:** Wren — run claudemd pipeline to regenerate role CLAUDE.md files; correct spec path.

## 4. CSC Compliance
**Status: GREEN**
`messages/scripts/` and `architect/scripts/` absent (spec paths N/A for this clone). Zero `/tmp/` violations in platform/scripts/ or proving/scripts/.
**Action:** None.

## 5. Git Dirty State
**Status: GREEN**
Working tree clean on main across all reachable directories. Latest commit: #3506 (wren) Jun 19. jeff-bridwell-personal-site, shared-observability, wordpress-blog not in this clone.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED**
No live board snapshot available; directing/products/cards/ contains only templates. True WIP state unknown; 18-day carry.
**Action:** Wren/Silas — refresh board snapshot or wire daily LaunchAgent capture. Blocked without host access.

## 7. Domain Context Freshness
**Status: YELLOW**
All 5 domain-context files updated Jun 15 (5 days ago, #3433/silas) — within 7-day window. Chorus cards continue shipping daily (#3506, #3511, #3513 all Jun 19-20); drift risk rising.
**Action:** Wren — update `domain-context-chorus.md` now; 2 more shipping days before threshold breach.

## 8. Disk Delta / Perf Baseline
**Status: YELLOW**
No perf-baseline time-series available in this environment. `com.chorus.perf-baseline.plist` exists but logs to `/tmp/`; no captured output reachable. 18-day carry.
**Action:** Run `platform/scripts/perf-baseline.sh` on host to establish baseline.
