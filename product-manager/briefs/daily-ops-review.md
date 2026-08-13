# Daily Ops Review — 2026-08-13

## 1. Hooks Health — YELLOW (day 11, carry)
`cargo check` not runnable from gathering-team (chorus-hooks lives in separate chorus repo). Carrying forward: 3 dead-code warnings stable — `owes_response_block` (ops.rs:178), `Liveness` enum (process.rs:64), `probe_role_session` (process.rs:76).
**Action:** Silas — clear dead paths before count grows; day 11.

## 2. LaunchAgent /tmp Refs — YELLOW (>13d carry)
20+ plists in `proving/config/launchagents/` still log to `/tmp/` (alert-notifier, api, clearing, context-cache ×3, fuseki, harvest-exporter, hooks, cruft-scan). No migration progress.
**Action:** Silas — confirm migration card exists and is assigned; >13d stall.

## 3. CLAUDE.md Fragments — GREEN (v1.6.0, no drift)
`designing/claudemd/` at v1.6.0; manifest.json present, ledger current. No version churn. Last bump Jul 25 (#3288).
**Action:** None. Monitor — bump expected before Aug 16 (Wren's prior commitment).

## 4. CSC Compliance — YELLOW (152 refs, -4 from yesterday)
152 `/tmp/` refs across 68 files in `platform/scripts/` (was 156, -4 refs today — some progress). `messages/scripts/` and `architect/scripts/` do not exist in this repo; check scoped to platform/scripts/.
**Action:** Silas — confirm what landed to clear 4 refs; 152 is the new floor.

## 5. Git Dirty State — GREEN
0 uncommitted changes. Recent cards: #3831, #3833, #3834, #3835, #3838, #3839, #3841, #3845.
**Action:** None.

## 6. Stale WIP Cards — YELLOW (123d zombie)
`roles/wren/state.json` card #1962 still "building" since 2026-04-12 (123d). Board snapshot access unresolved.
**Action:** Wren — close state.json #1962 artifact; it's a ghost.

## 7. Domain Context Freshness — RED (breach today)
Morning summary confirms breach today (Aug 13). Last commit to domain-context files: Aug 10 (ad25367). Chorus and infrastructure domains most active — 8+ cards shipped since Aug 7.
**Action:** Wren — update domain-context-chorus.md + domain-context-infrastructure.md NOW; protocol breach.

## 8. Disk Delta — N/A (day 63 carry)
No `perf-baseline-*.json` committed. Scripts exist (`platform/scripts/perf-baseline.sh`, `perf-baseline-chorus.sh`) but emit no tracked output.
**Action:** Silas — commit baseline to `platform/state/` or close this lane; 63d unresolved.
