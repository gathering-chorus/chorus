# Daily Ops Review — 2026-04-05

> Feeds Wren's morning summary. Each section: status · finding · action.

---

## 1. Hooks Health
**YELLOW** — `cargo check` passes (Finished, 0 errors) but emits **30 warnings**; 9 are auto-fixable. Dead code: `decision_block_json` at `chorus/platform/services/chorus-hooks/src/types.rs:175`.
**Action:** Run `cargo fix --bin "chorus-hooks" -p chorus-hooks`; wire or remove `decision_block_json`.

## 2. LaunchAgent /tmp Refs
**YELLOW** — 10+ plists (chorus-hooks, clearing, ops, context-cache ×3, fuseki-perf, posture-capture, harvest-exporter, api) route stdout/stderr to `/tmp/`. Logs lost on reboot — pattern systemic.
**Action:** Migrate log paths to `~/Library/Logs/chorus/`; Silas to own.

## 3. CLAUDE.md Conflicts
**YELLOW** — Two source fragments updated today (2026-04-05): `communication-discipline.md` and `infrastructure-operations-reference.md`. `chorus/CLAUDE.md` not regenerated (last touched 2026-04-03).
**Action:** Re-run CLAUDE.md assembly script to sync root file with updated fragments.

## 4. CSC Compliance
**GREEN** — No `/tmp/` refs found in `messages/scripts/` or `architect/scripts/`.
**Action:** None.

## 5. Git Dirty State
**GREEN** — All 7 role directories clean. Repo is fully committed.
**Action:** None.

## 6. Stale WIP Cards
**RED** — `card-1865` in WIP since 2026-03-31 (5 days). Kade's WIP backlog has 10 cards stuck since 2026-03-14 to 2026-03-16 (20+ days).
**Action:** Wren to triage with engineer/Kade — close, defer, or re-estimate. No WIP card >48h without a daily touch.

## 7. Domain Context Freshness
**GREEN** — All 5 domain-context files updated within 7 days (infrastructure, music, photos, seeds: 2026-04-03; chorus: 2026-04-05). Last shipped cards are 2026-04-02.
**Action:** None.

## 8. Disk Delta
**RED** — Used storage jumped from **257 GB** (2026-03-29) to **1,027 GB** (2026-04-04): **+299.7% in 6 days**. `percentUsed` field in perf-baseline JSON is corrupt (shows 2% vs actual ~51%). Gap in baseline data: no run between 2026-03-29 and 2026-04-04.
**Action:** Identify source of ~770 GB growth immediately (suspect Fuseki journal, log accumulation, or model artifacts). Fix `percentUsed` calculation in `chorus/platform/scripts/perf-baseline.sh`. Restore nightly baseline cadence.
