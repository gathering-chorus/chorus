# Daily Ops Review — 2026-05-15

## 1. Hooks Health
**Status: YELLOW**
`cargo check` passes (35s) with 7 warnings (4 duplicates). Two substantive dead-code warnings:
- `load_role_sections` in `src/shared/protocol_contract.rs:155` — unused function
- `chorus_worktree_override` field in `src/types.rs:55` — unread struct field

**Action:** File cleanup card or silence with `#[allow(dead_code)]` if intentionally deferred.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
Two plist files write to `/tmp`:
- `platform/services/chorus-hooks/com.chorus.hooks.plist` — stdout/stderr logs to `/tmp/chorus-hooks.{stdout,stderr}.log`
- `config/launchagents/com.chorus.tmp-reaper.plist` — expected (it is the tmp reaper, not a violation)

**Action:** Redirect chorus-hooks daemon logs to `~/Library/Logs/Chorus/` to match other agents; file as CSC hygiene card.

## 3. CLAUDE.md Fragment Staleness
**Status: GREEN**
`designing/claudemd/` has `PROTOCOL_VERSION=1.4`, manifest `_build=217`, and full shared + per-role fragment tree (kade/silas/wren). Last pipeline-run artifacts are from 2026-02-21, but #2731 (CLAUDE.md as derived artifact) shipped 2026-05-05 — fragments are now the canonical source, generated output is expected not to live here.

**Action:** None. Pipeline-run timestamps are pre-#2731 artifact; no staleness concern.

## 4. CSC Compliance (/tmp/ in Scripts)
**Status: RED**
Multiple `platform/scripts/` files use hardcoded `/tmp/` paths without env override:
- `look.sh` — `CAPTURE_DIR="/tmp/chorus-look"`
- `coherence-check` — `PULSE_FILE` and `STATE_DIR` hardcoded to `/tmp/`
- `bedroom-heartbeat.sh` — state + log files hardcoded to `/tmp/`
- `bridge-subscriber.js` — `INBOX_DIR` hardcoded to `` `/tmp/voice-inbox/${role}` ``
- `index-crawler-snapshots.sh` — status + run files hardcoded to `/tmp/`

`bridge-subscriber-watchdog.sh` and `werk-init.sh` have partial env overrides — acceptable.

**Action:** File CSC-compliance sweep card for hardcoded `/tmp/` in operational scripts; priority on `bridge-subscriber.js` (runtime) and `coherence-check` (pulse path).

## 5. Git Dirty State
**Status: GREEN**
`git status` is clean — no uncommitted changes in gathering-team repo. External repos (jeff-bridwell-personal-site, shared-observability, wordpress-blog) not reachable in this environment; verify locally if needed.

**Action:** None for this repo. Spot-check peer repos at next standup.

## 6. Stale WIP Cards
**Status: YELLOW**
`roles/kade/current-work.md` last updated 2026-04-18 — 27 days stale, still shows #2180 as WIP. Most recent commits (May 13–15) are on #2923/#2913/#2915, suggesting active work is not reflected in that file. No board query available in this environment.

**Action:** Kade to refresh `current-work.md` or confirm #2180 is parked/dropped. Wren to surface in morning summary if unresolved.

## 7. Domain Context Freshness
**Status: YELLOW**
All domain-context files last touched 2026-05-10 (5 days ago). Cards #2923, #2913, #2915 shipped in the chorus/infrastructure domain since then — contexts not yet updated.

- `domain-context-chorus.md` — 5d old, active shipping
- `domain-context-infrastructure.md` — 5d old, active shipping

Under the 7-day threshold today; will breach tomorrow if not refreshed.

**Action:** Kade or Silas to update chorus + infrastructure contexts before next session, or flag if the domain is stable enough to skip.

## 8. Disk Delta
**Status: N/A**
No perf-baseline snapshot available in this environment. `config/launchagents/com.chorus.perf-baseline.plist` exists but runtime data is local-only.

**Action:** Run perf-baseline comparison locally; no remote data to diff here.
