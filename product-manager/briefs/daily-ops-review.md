# Daily Ops Review — 2026-07-28

## 1. Hooks Health
**Status: YELLOW→IMPROVING**
`cargo check` passes. Down to **1** dead-code warning: `owes_response_block` (nudge_drain.rs:178). `registration_json` warning from Jul 27 is resolved.
**Action:** Silas — suppress or remove `owes_response_block`; good progress, one item remaining.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
34 plists across repo reference `/tmp/` (17 in `proving/config/launchagents/`, remainder in `platform/scripts/launchagents-*`). Count unchanged from Jul 27.
**Action:** Silas — migration card still open; no movement.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry)**
Shared fragments in `designing/claudemd/shared/` last committed 2026-07-23 (5d). Root `CLAUDE.md` updated today. Fragment sync lag unresolved; `chorus-prompt.md` is the only current file.
**Action:** Wren + Kade — role fragments still need content refresh post Jul 27 cards.

## 4. CSC Compliance
**Status: RED (carry)**
`messages/scripts/` and `architect/scripts/` paths do not exist. `platform/scripts/` has **37 .sh files** with `/tmp/` refs — count unchanged from Jul 26.
**Action:** Silas — remediation card still needed; check-path mismatch is a separate tracking gap.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes across all tracked role directories.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry +1d)**
2 WIP cards last updated 2026-04-07 — now **112d** stale (was 111d). Cards: "Framework service design — OWL entity model" and "Restore chorus product boundary". Appear in all 3 role board snapshots.
**Action:** Jeff/Wren — close or archive #1759/#1791; 112d in WIP is a board-health blocker.

## 7. Domain Context Freshness
**Status: RED (carry)**
All 5 domain-context files at 2026-07-23 (5d old). Cannot determine cards shipped today without live Vikunja access; Jul 27 showed 4 chorus/infra cards shipped, making chorus context ~100d stale.
**Action:** Silas — chorus context critical; Wren — all 5 files overdue for refresh.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` in `proving/logs/`. Scripts exist but no prior run data to diff against. Repo total: 664 MB.
**Action:** Silas — land nightly baseline JSON to `proving/logs/` to enable delta tracking.

---
*Jul 28 delta: §1 improved (2→1 warning, `registration_json` resolved). §6 escalated to 112d. All other items carry from Jul 27. Escalation: §7 chorus context ~100d stale; §6 WIP cards 112d dead.*
