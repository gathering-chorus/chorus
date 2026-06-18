# Daily Ops Review — 2026-06-18

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes; 8 dead-code warnings unchanged since 2026-06-02 (16-day carry). `load_role_sections` (protocol_contract.rs:155) and `chorus_worktree_override` (types.rs:64) still unresolved.
**Action:** Silas — suppress with `#[allow(dead_code)]` or delete; escalate to RED at next weekly.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
`com.chorus.tmp-reaper.plist` clean (expected). Secondary plists in `platform/scripts/launchagents-secondary/` route logs to `/tmp/` (images-api-server, images-api-video, chorus-ops). Logs lost on reboot; count unchanged from Jun 16.
**Action:** Migrate StandardOut/Err to `~/Library/Logs/Chorus/`; 16-day carry with no movement.

## 3. CLAUDE.md Fragments
**Status: YELLOW**
Role CLAUDE.md files not generated into `roles/*/` — last pipeline run 2026-02-21. Fragments (9 per role) last touched Jun 5. No divergence detectable without generated output present; pipeline may be stale.
**Action:** Wren — run claudemd pipeline to regenerate role CLAUDE.md files; pipeline-runs dir has nothing since Feb.

## 4. CSC Compliance
**Status: GREEN**
`messages/scripts/` and `architect/scripts/` absent — spec paths N/A. `platform/scripts/` `/tmp/` refs are runtime-state patterns (markers, sockets, test fixtures); no new violations since Jun 16.
**Action:** None.

## 5. Git Dirty State
**Status: GREEN**
Working tree clean on main. All role dirs in scope (product-manager, architect, wren, silas, kade) committed. Other repos (jeff-bridwell-personal-site, shared-observability, wordpress-blog) not reachable from this clone.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED**
Board snapshots are **72 days stale** (last refreshed 2026-04-07). Same two stale WIP cards: "Framework service design — OWL entity model" (~1711h) and "Restore chorus product boundary" (~1707h). True board state unknown; 16-day carry.
**Action:** Wren/Silas — refresh board snapshot; wire daily LaunchAgent capture. Blocked without host access.

## 7. Domain Context Freshness
**Status: RED**
`domain-context-chorus.md` last updated 2026-06-05 (13 days) — breaches 7-day threshold while 8+ chorus/owl-api cards shipped this week (#3435, #3467, #3468, #3453). `domain-context-music.md`, `-photos.md`, `-seeds.md` also 13 days stale. `domain-context-infrastructure.md` current (Jun 18).
**Action:** Wren — `domain-context-chorus.md` is highest priority; music/photos/seeds need owner assignment.

## 8. Disk Delta / Perf Baseline
**Status: YELLOW**
No perf-baseline time-series available in this environment. `com.chorus.perf-baseline.plist` exists but logs to `/tmp/`; baseline script never captured output here. Delta cannot be computed; 16-day carry.
**Action:** Run `platform/scripts/perf-baseline.sh` on host to establish baseline.
