# Daily Ops Review — 2026-06-28

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (38s) with 8 warnings (1 duplicate). Warning count grew from 7 → 8 vs last review.
Dead-code: 5 unused functions (`find_most_recent_pending`, `handle_approval_request`, `is_demo_or_done`, `has_test_run`, `has_production_code_edit`), 1 unused import, 2 unread fields (`chorus_worktree_override`, `at_step` x3).

**Action:** File cleanup card. `chorus_worktree_override` and `at_step` may signal abandoned feature paths, not just incidental noise.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
36 plist files in `proving/config/launchagents/` write stdout/stderr to `/tmp/`. Scope has grown since May review (was 2 named files). Pattern is now structural across all daemon agents.

**Action:** CSC hygiene sweep for plist log paths — redirect to `~/Library/Logs/Chorus/`. Scope is large enough to warrant a card rather than spot fixes.

## 3. CLAUDE.md Fragment Staleness
**Status: N/A**
`messages/claudemd/` and `designing/claudemd/` directories not present in this repo. Only root `CLAUDE.md` exists.

**Action:** Verify correct path on host; confirm whether claudemd fragments are local-only artifacts excluded from git.

## 4. CSC Compliance
**Status: RED**
20+ files in `platform/scripts/` have hardcoded `/tmp/` paths. Unchanged from May review — no remediation has shipped. Top offenders: `bridge-subscriber.js` (runtime inbox), `coherence-check` (pulse/state), `bedroom-heartbeat.sh`, `look.sh`, `index-crawler-snapshots.sh`.

**Action:** CSC sweep card unresolved since 2026-05-15. Assign owner or timebox for 2026-07. `bridge-subscriber.js` is highest risk (runtime, role-scoped paths).

## 5. Git Dirty State
**Status: GREEN**
Repo is clean — 0 uncommitted changes in gathering-team. Peer repos (jeff-bridwell-personal-site, shared-observability, wordpress-blog) not accessible from this environment.

**Action:** Spot-check peer repos locally at standup.

## 6. Stale WIP Cards
**Status: YELLOW**
`roles/kade/current-work.md` last committed 2026-06-21 (7 days). Still reflects #2180 (server.ts handler extraction, 39/~100 done) as active WIP — but recent commits show #3580–#3596 as the live work. `roles/wren/next-session.md` also stale since 2026-06-21.

**Action:** Kade to refresh `current-work.md`; #2180 should be updated with current progress or marked parked. Wren to flag in morning summary.

## 7. Domain Context Freshness
**Status: RED**
All 5 domain-context files (`chorus`, `infrastructure`, `music`, `photos`, `seeds`) last committed 2026-06-21 — 7 days ago today, crossing threshold. 7+ cards shipped in chorus/infrastructure/code domains since (#3580, #3581, #3586, #3587, #3590, #3593, #3596).

**Action:** `domain-context-chorus.md` and `domain-context-infrastructure.md` need updates today. Silas (#3593) and Kade (#3596) are the relevant owners.

## 8. Disk Delta
**Status: N/A**
No perf-baseline data available in this environment. LaunchAgent `com.chorus.perf-baseline.plist` exists but runtime snapshots are local-only.

**Action:** Run perf-baseline comparison on host machine; no remote data to diff here.
