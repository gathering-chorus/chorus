# Daily Ops Review — 2026-06-15

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes; 8 dead-code warnings unchanged since 2026-06-02 (13-day carry). `load_role_sections` (protocol_contract.rs:155) and `chorus_worktree_override` (types.rs:64) still unresolved.
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; escalate to RED after 7 days at same count.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
33 `/tmp` refs across 19 plists: 17 in `proving/config/launchagents/`, 2 in `platform/scripts/launchagents-secondary/`. Count up from "20+ across 8" on Jun 3 — scope expanded. All are log paths (StandardOut/Err), not operational data; logs lost on reboot.
**Action:** Migrate to `~/Library/Logs/Chorus/`; 13-day carry with no movement.

## 3. CLAUDE.md Fragments
**Status: GREEN**
21 fragments in `designing/claudemd/shared/`. No divergence detected. `messages/claudemd/` still absent — scope remains N/A.
**Action:** None.

## 4. CSC Compliance
**Status: GREEN**
`messages/scripts/` and `architect/scripts/` absent — spec paths N/A. `platform/scripts/` has 9 `/tmp/` refs (coherence-check, werk-init.sh, look.sh, bridge-subscriber.js, etc.) but these are runtime-state patterns, not CSC violations.
**Action:** None. Confirm platform/scripts/ scope with Silas if CSC definition broadens.

## 5. Git Dirty State
**Status: GREEN**
Working tree clean on main. `product-manager/` 0 uncommitted changes. Other role dirs live in separate repos — not checkable here.
**Action:** None in scope.

## 6. Stale WIP Cards
**Status: YELLOW**
`directing/vikunja/cards/` is empty — no WIP card files to diff. No board snapshot available to verify against. Jun 3 review flagged this as RED; no snapshot still captured.
**Action:** Capture board snapshot; wire daily refresh via LaunchAgent or chorus-ops (carried 12 days).

## 7. Domain Context Freshness
**Status: RED**
`domain-context-chorus.md`, `domain-context-music.md`, `domain-context-photos.md`, `domain-context-seeds.md` last updated **2026-06-05 (10 days ago)** — all breach the 7-day threshold. `domain-context-infrastructure.md` current (Jun 15). Chorus domain shipped 8+ cards since Jun 5.
**Action:** Wren — refresh `domain-context-chorus.md` today; assign music/photos/seeds updates.

## 8. Disk Delta / Perf Baseline
**Status: YELLOW**
`data/athena/tree.json` present but no perf-baseline series. `com.chorus.perf-baseline.plist` logs to `/tmp` but baseline never captured. Cannot compute disk delta. Carried 13 days.
**Action:** Run `platform/scripts/perf-baseline.sh` on host; action overdue.
