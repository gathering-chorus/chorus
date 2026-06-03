# Daily Ops Review — 2026-06-03

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (0 errors); 8 warnings unchanged from 2026-06-02. Dead-code: `load_role_sections` never called (`protocol_contract.rs:155`), `chorus_worktree_override` field never read (`types.rs:55`).
**Action:** Dead-code cleanup card still open; file a follow-up if warnings don't drop in next 2 reviews.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
20+ `/tmp` refs across 8 plists (context-cache, metrics, harvest-exporter, clearing, ops, cruft-scan, alert-notifier, fuseki-*). All are `StandardOutPath`/`StandardErrorPath` log paths — not operational data. Logs lost on reboot.
**Action:** Known gap — migrate to `~/Library/Logs/Chorus/`; unchanged since last review.

## 3. CLAUDE.md Conflicts
**Status: GREEN**
`designing/claudemd/` present with `roles/`, `shared/`, `pipeline-runs/` subdirs + manifest. No diff conflicts detected. (Note: `messages/claudemd/` path from check spec does not exist — canonical location is `designing/claudemd/`.)
**Action:** None; confirm spec path in next ops review cycle.

## 4. CSC Compliance (/tmp in scripts)
**Status: GREEN**
No `/tmp/` references in `messages/scripts/` or `architect/scripts/` (neither directory present in this clone; check is N/A for this repo).
**Action:** None.

## 5. Git Dirty State
**Status: GREEN**
`product-manager/` (only role dir in this repo): 0 uncommitted changes. Remaining 6 role dirs are in separate repos — not checkable from here.
**Action:** Extend check scope to cover cross-repo dirty state via chorus-ops or CI.

## 6. Stale WIP Cards
**Status: RED**
No board snapshot data accessible in this repo. Yesterday's brief noted snapshots 56 days old (last 2026-04-07). Issue unresolved.
**Action:** Capture board snapshot now; wire daily refresh via LaunchAgent or chorus-ops.

## 7. Domain Context Freshness
**Status: GREEN**
All 5 domain-context files 4 days old (chorus, infrastructure, music, photos, seeds) — within 7-day threshold. Cards #3195/#3191/#3192/#3185/#3187 shipped in chorus/infrastructure domains; domain context not yet stale.
**Action:** Recheck after 3 more days if no domain-context update committed.

## 8. Disk Delta / Perf Baseline
**Status: GREY**
`perf-baseline.sh` and `perf-baseline-chorus.sh` exist; no captured baseline output. Cannot compute delta.
**Action:** Run `platform/scripts/perf-baseline.sh`, commit output to establish baseline (carried from 2026-06-02).
