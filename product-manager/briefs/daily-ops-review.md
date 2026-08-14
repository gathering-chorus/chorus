# Daily Ops Review — 2026-08-14

## 1. Hooks Health — YELLOW (day 12, warning set expanded)
`cargo check` passes; **4 warnings** (up from 3 stable). New warning: `clear_cap_cache` (word_cap.rs:163). Stable set (`owes_response_block` ops.rs:178, `Liveness` process.rs:64, `probe_role_session` process.rs:76) now has a 4th — set is no longer stable.
**Action:** Silas — new dead path introduced; address the expanding set before day 14.

## 2. LaunchAgent /tmp Refs — YELLOW (>14d carry)
20+ plists in `proving/config/launchagents/` still log to `/tmp/` (alert-notifier, api, clearing, context-cache ×3, fuseki, harvest-exporter, hooks, cruft-scan). No migration progress.
**Action:** Silas — migration card must exist and be assigned; >14d stall is a protocol breach.

## 3. CLAUDE.md Fragments — YELLOW (v1.6.0, 2d to commitment)
`designing/claudemd/` at v1.6.0; last bump Jul 25 (#3288). Wren committed to refresh before Aug 16 — 2 days remaining. No fragment churn today.
**Action:** Wren — fragment bump due Aug 16; 2 days.

## 4. CSC Compliance — RED (152 refs, unchanged)
152 `/tmp/` refs in `platform/scripts/` — no change from yesterday. (`messages/scripts/`, `architect/scripts/` do not exist; check is platform-scoped.)
**Action:** Silas — 152 is unchanged floor; assign migration card or escalate.

## 5. Git Dirty State — GREEN
0 uncommitted changes. Active cards landed today: #3865, #3862, #3863, #3845, #3861.
**Action:** None.

## 6. Stale WIP Cards — YELLOW (124d zombie)
`roles/wren/state.json` card #1962 still "building" since 2026-04-12 (124d). Board snapshot access still unresolved.
**Action:** Wren — close state.json #1962 ghost artifact; this is now month 4.

## 7. Domain Context Freshness — RED (breach day 2)
Morning summary confirms "domain-context breach d2." File headers are authoritative: chorus last updated 2026-04-19, infrastructure/music/photos 2026-03-25/26, seeds 2026-04-01 — all months stale. 10+ chorus/infra cards shipped since April with no context update.
**Action:** Wren — update domain-context-chorus.md + domain-context-infrastructure.md immediately; breach escalates daily.

## 8. Disk Delta — N/A (day 64 carry)
No `perf-baseline-*.json` committed. Scripts exist but emit no tracked output. Cannot compute delta.
**Action:** Silas — commit baseline to `platform/state/` or formally close this lane; 64d unresolved.
