# Daily Ops Review — 2026-06-16

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes; 8 dead-code warnings unchanged since 2026-06-02 (14-day carry). `load_role_sections` (protocol_contract.rs:155) and `chorus_worktree_override` (types.rs:64) still unresolved.
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; escalate to RED after next weekly without movement.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
33 `/tmp` refs across 17 plists in `proving/config/launchagents/`. All are log paths (StandardOut/Err), not operational data — logs lost on reboot. Count unchanged from Jun 15.
**Action:** Migrate to `~/Library/Logs/Chorus/`; 14-day carry with no movement.

## 3. CLAUDE.md Fragments
**Status: GREEN**
24 fragments in `designing/claudemd/shared/`. 3 updated today (cross-machine-operations-core.md, cross-machine-operations-reference.md, portfolio-reference.md). No divergence detected; `messages/claudemd/` path still absent (N/A).
**Action:** None.

## 4. CSC Compliance
**Status: GREEN**
`messages/scripts/` and `architect/scripts/` absent — spec paths N/A. `platform/scripts/` has 65 files with `/tmp/` refs but these are runtime-state patterns (markers, temp configs), not CSC violations.
**Action:** None.

## 5. Git Dirty State
**Status: GREEN**
Working tree clean on main. All committed. Other role dirs (jeff-bridwell-personal-site, shared-observability, wordpress-blog) are separate repos not reachable from this clone.
**Action:** None in scope.

## 6. Stale WIP Cards
**Status: RED**
Board snapshots are **70 days stale** (last refreshed 2026-04-07). Two WIP cards visible: "Framework service design — OWL entity model" and "Restore chorus product boundary", both last touched Apr 7. Cannot verify real board state; true WIP count unknown.
**Action:** Wren/Silas — refresh board snapshot today; wire daily snapshot via LaunchAgent. Carried 14 days.

## 7. Domain Context Freshness
**Status: RED**
`domain-context-chorus.md`, `domain-context-music.md`, `domain-context-photos.md`, `domain-context-seeds.md` last updated **2026-06-05 (11 days ago)** — all breach the 7-day threshold. 18+ cards shipped across domains since Jun 9. `domain-context-infrastructure.md` current (Jun 16).
**Action:** Wren — refresh `domain-context-chorus.md` today; assign music/photos/seeds updates.

## 8. Disk Delta / Perf Baseline
**Status: YELLOW**
`data/athena/tree.json` present (product model only); no perf-baseline time-series captured. `com.chorus.perf-baseline.plist` exists but logs to `/tmp` and has never run successfully. Disk delta cannot be computed.
**Action:** Run `platform/scripts/perf-baseline.sh` on host machine; carried 14 days.
