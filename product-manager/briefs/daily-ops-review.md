# Daily Ops Review — 2026-05-30

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (no errors) but emits 15 dead-code warnings across `chorus-hooks`: unused functions (`load_role_sections`, `check`, `read_role_card`, `sweep_stale_pending`, etc.) and unused structs (`PendingPayload`, `PendingOpts`, `ApprovalSignal`).
**Action:** Schedule a cleanup card to prune dead code — noise masks real regressions.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
14+ plists in `proving/config/launchagents/` write logs to `/tmp/` (hooks, ops, api, context-cache, clearing, posture-capture, fuseki-perf, cruft-scan). `/tmp` is purged on macOS reboot.
**Action:** Migrate log paths to `$CHORUS_HOME/logs/` or `~/Library/Logs/` — this is a known CSC compliance gap.

## 3. CLAUDE.md Conflicts
**Status: GREEN**
`messages/claudemd/` directory does not exist in this repo (single-repo layout). Only one `CLAUDE.md` found at repo root, last committed 2026-05-25.
**Action:** None — no fragment staleness to assess.

## 4. CSC Compliance (/tmp in scripts)
**Status: GREEN**
No `/tmp/` references found in `messages/scripts/` or `architect/scripts/` (neither directory exists in this clone).
**Action:** None — clean in scope.

## 5. Git Dirty State
**Status: GREEN**
Repo working tree is clean (0 uncommitted changes). Only `product-manager/` role directory present in this clone; 6 of 7 expected role dirs (`architect`, `engineer`, `messages`, `jeff-bridwell-personal-site`, `shared-observability`, `wordpress-blog`) are absent — consistent with a remote clone of gathering-team only.
**Action:** None — expected for this repo boundary.

## 6. Stale WIP Cards
**Status: YELLOW (data stale)**
Board snapshot is from 2026-04-07 (53 days old). At snapshot time: 2 WIP cards, both < 6h old relative to snapshot. Cannot confirm current state.
- `#1759` Framework service design — OWL entity model (Wren)
- `#1791` Restore chorus product boundary (Silas)
**Action:** Refresh board snapshot — `platform/logs/board-snapshot-gathering-*.json` need re-capture.

## 7. Domain Context Freshness
**Status: YELLOW**
All 5 `domain-context-*.md` files last committed 2026-05-25 (5 days ago). Within 7-day window today, but approaching threshold. Cannot verify against cards shipped since then without live board data.
**Action:** Re-evaluate after board snapshot refresh; flag if any domain had cards shipped since 05-25.

## 8. Disk Delta / Perf Baseline
**Status: GREY (no baseline data)**
`perf-baseline.sh` and `perf-baseline-chorus.sh` scripts exist but no captured baseline output found in repo.
**Action:** Run `platform/scripts/perf-baseline.sh` to establish a baseline; check `~/.chorus/` for prior runs.
