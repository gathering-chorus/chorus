# Daily Ops Review — 2026-08-12

## 1. Hooks Health — YELLOW (day 10, warning set stable)
`cargo check` passes; 3 dead-code warnings unchanged from yesterday: `owes_response_block` (ops.rs:178), `Liveness` enum (process.rs:64), `probe_role_session` (process.rs:76) — introduced with #3810/#3811.
**Action:** Silas — 3 dead paths, day 10; clear before count grows.

## 2. LaunchAgent /tmp Refs — YELLOW (>12d carry)
20+ plists in `proving/config/launchagents/` still write to `/tmp/` (alert-notifier, api, clearing, context-cache ×3, fuseki, harvest-exporter, hooks, cruft-scan, etc.). No migration progress.
**Action:** Silas — confirm migration card exists and is assigned; >12d stall.

## 3. CLAUDE.md Fragments — YELLOW (v1.6.0, ledger 2026-08-03)
`designing/claudemd/` at v1.6.0; manifest.json present. No version churn since last confirmed ledger date (Aug 3, 9d ago). Fragment staleness acceptable but approaching floor.
**Action:** Wren — confirm CLAUDE.md fragments still reflect current protocol surface before Aug 16.

## 4. CSC Compliance — RED (156 refs, unchanged)
156 `/tmp/` refs across 68 files in `platform/scripts/`. Confirmed today (correct cwd). Same count as yesterday — no migration progress. Hot spots: coherence-check, look.sh, bridge-subscriber-watchdog.sh, werk-init.sh, bedroom-heartbeat.sh.
**Action:** Silas — assign migration card; 156 is the floor to beat.

## 5. Git Dirty State — GREEN
0 uncommitted changes. Cards landed today: #3819, #3813, #3825, #3827, #3826.
**Action:** None.

## 6. Stale WIP Cards — YELLOW (board-snapshot gap + 122d zombie)
Board snapshots still 0-byte (unresolved). `roles/wren/state.json` card #1962 in "building" since 2026-04-12 (122d). Open PRs are Dependabot-only (#449/#443 at 71d; batch #838–#845 at 11d).
**Action:** Wren — close state.json #1962 artifact. Jeff — decide on #449/#443. Silas — diagnose empty board-snapshot files.

## 7. Domain Context Freshness — RED (breach tomorrow)
All 5 `designing/domain-context/` files last committed 2026-08-06 (6d ago); 7d threshold hits Aug 13. Chorus domain had 7+ cards since that date (#3807, #3810, #3813, #3819, #3821, #3825, #3827).
**Action:** Wren — update domain-context-chorus.md TODAY; infrastructure.md close behind.

## 8. Disk Delta — N/A (day 62+ carry)
No `perf-baseline-*.json` committed. `platform/scripts/perf-baseline.sh` exists but emits no tracked output. Cannot compute delta.
**Action:** Silas — decide: commit baseline snapshot to `platform/state/` or close this lane.
