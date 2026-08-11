# Daily Ops Review — 2026-08-11

## 1. Hooks Health
**Status: YELLOW (day 9, warning set changed)**
`cargo check` passes; 3 dead-code warnings — net increase from 2 yesterday. Old set (`registration_json` session_registry.rs:66, `owes_response_block` nudge_drain.rs:178) replaced by: `owes_response_block` (ops.rs:178), `Liveness` enum (process.rs:64), `probe_role_session` (process.rs:76). Likely from #3810/#3811 refactor landing today.
**Action:** Silas — 2 new dead paths introduced; address before warning set grows further (day 9).

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, >11d)**
20+ plist files in `proving/config/launchagents/` still log to `/tmp` (alert-notifier, api, clearing, context-cache ×3, fuseki-compact/perf, harvest-exporter, hooks, cruft-scan, etc.). No migration progress.
**Action:** Silas — confirm migration card exists and is assigned; >11d stall.

## 3. CLAUDE.md Fragments
**Status: YELLOW (breach Aug 13)**
`designing/claudemd/` at v1.6.0 (ledger: 2026-08-03). Domain-context files last committed 2026-08-06 (#3757 wren). Today is 5 days since update; 7d breach = Aug 13. Note: yesterday's review stated "Aug 4" — git log corrects that to Aug 6, giving 2 more days.
**Action:** Wren — update domain-context-chorus.md and domain-context-infrastructure.md before Aug 13.

## 4. CSC Compliance
**Status: RED (156 refs, unchanged)**
156 `/tmp/` refs across 68 files in `platform/scripts/`. Same count as yesterday — no progress. Known hot spots: coherence-check, bridge-subscriber-watchdog.sh, look.sh, nightly-suites.sh, bedroom-heartbeat.sh, werk-init.sh.
**Action:** Silas — assign migration card; 156 is the stable-red ceiling to beat.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes. Today's landed cards: #3810 (wren ×2 PRs #912/#914), #3811 (kade #913). Repo clean.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW (Dependabot + stale role state)**
Board snapshots empty (0 bytes — unresolved). `roles/wren/state.json` shows card #1962 in "building" since 2026-04-12 (121d) — likely stale state.json never closed out. No new human WIP cards visible in PRs (all open PRs are Dependabot: #449/#443 at 70d; batch #838–#845 at 10d).
**Action:** Wren — close out state.json card #1962 artifact. Jeff — decision on #449/#443. Silas — board-snapshot empty file needs investigation.

## 7. Domain Context Freshness
**Status: YELLOW (breach Aug 13)**
All 5 domain-context files (chorus, infrastructure, music, photos, seeds) last committed 2026-08-06. Active work in chorus domain (#3807, #3810) and infrastructure (#3776) since that date. 5d stale; threshold at 7d.
**Action:** Wren — update domain-context-chorus.md today given chorus card volume.

## 8. Disk Delta
**Status: N/A (carry, day 61+)**
No `perf-baseline-*.json` committed artifacts. `platform/scripts/perf-baseline.sh` exists but emits no tracked output. Cannot compute delta.
**Action:** Silas — decide: commit baseline snapshot to `platform/state/` or close this lane.
