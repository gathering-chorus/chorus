# Daily Ops Review — 2026-08-06

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passed — no errors, 2 dead-code warnings (`registration_json` in session_registry.rs:66, `owes_response_block` in nudge_drain.rs:178). Both unused-pub functions, not regressions.
**Action:** None urgent. Consider pruning dead-code warnings in a housekeeping card.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
11+ plists in `proving/config/launchagents/` use `/tmp/` for log paths (alert-notifier, api, clearing, context-cache ×3, cruft-scan, fuseki ×2, harvest-exporter, hooks). Persistent since July review — no remediation shipped.
**Action:** CSC hygiene card still open. Redirect log paths to `~/Library/Logs/Chorus/`.

## 3. CLAUDE.md Fragment Staleness
**Status: YELLOW → RED**
All 24 fragments in `designing/claudemd/shared/` last updated 2026-07-31 — breach 7-day threshold today. Wren's morning summary (today) explicitly flags this. Domain-context files also at 7d boundary (see §7).
**Action:** Fragment refresh needed today. Wren owns. npm ci staleness (day 58, no owner) surfaced in same summary — assign.

## 4. CSC Compliance (/tmp in scripts)
**Status: YELLOW**
`platform/scripts/` has 14+ `/tmp/` usages across: `coherence-check`, `look.sh`, `bridge-subscriber-watchdog.sh`, `nightly-suites.sh`, `bedroom-heartbeat.sh`, `werk-init.sh`, `crawler-hydrate-graph.sh`. Also `roles/kade/scripts/` has 5 hits (`gen-thumbs-bedroom.py`, `wm-schema-extract.py`, `photo-pipeline.py`, `gen-video-thumbs-bedroom.py`). No architect scripts affected.
**Action:** Known ongoing. `/tmp/` in operational scripts acceptable if ephemeral; LaunchAgent log paths are the priority (§2).

## 5. Git Dirty State
**Status: GREEN**
Working tree clean across all role directories. No uncommitted changes detected.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW**
Two Wren cards stalled >48h: **#3724** (path-routed /domains) — built but NOT on main since 2026-08-03, land verb stalled twice reporting `running` with no process; **#3718** (TBox verbs) — 53 tests passing, AC4 gate unarmed since 2026-08-03. Both blocked on Silas (land-verb substrate / model governance).
**Action:** Silas to unblock land-verb stall (#3724) and arm AC4 gate (#3718). Wren confirm still blocked.

## 7. Domain Context Freshness
**Status: RED**
All 5 domain-context files (`chorus`, `infrastructure`, `music`, `photos`, `seeds`) last updated 2026-07-31 — hit 7-day ceiling today. Active shipments in chorus domain this week (#3724, #3718, Kade's nightly-to-zero #3606, #3758). Context files have not tracked these changes.
**Action:** Refresh domain-context files today, prioritizing `chorus` and `infrastructure`. Wren owns per morning summary flag.

## 8. Disk Delta
**Status: N/A**
No perf-baseline data found in repo (`data/`, `.coverage-denominator-baseline` present but not disk growth data). Cannot compute delta.
**Action:** Consider establishing a disk-delta baseline artifact in `data/` if this check is recurring.
