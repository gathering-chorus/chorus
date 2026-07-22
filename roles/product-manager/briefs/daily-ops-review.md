# Daily Ops Review — 2026-07-22

## 1. Hooks Health
**Status: N/A**
`messages/services/chorus-hooks` not present in this environment (host-local Cargo workspace). Cannot run `cargo check` remotely.

**Action:** Run on host; prior review (2026-06-28) showed 8 warnings — track trend there.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW**
18 plist files in `config/launchagents/` + `proving/config/launchagents/` reference `/tmp/`. 33 in proving alone. Unchanged since June review — no remediation shipped.

**Action:** CSC hygiene card still open. Plist log paths should redirect to `~/Library/Logs/Chorus/`.

## 3. CLAUDE.md Fragment Staleness
**Status: GREEN**
24 fragments in `designing/claudemd/shared/` all last updated 2026-07-17 (5 days ago). Within threshold.

**Action:** None today. Revisit if no update by 2026-07-24.

## 4. CSC Compliance
**Status: RED**
15+ files in `platform/scripts/` have hardcoded `/tmp/` paths. Unchanged since May 2026 — no remediation has shipped across 6 weekly reviews. Top offenders: `bridge-subscriber.js` (runtime inbox), `coherence-check` (pulse+state), `bedroom-heartbeat.sh`, `look.sh`.

**Action:** Assign owner or formally accept risk. `bridge-subscriber.js` is highest-risk (runtime, role-scoped paths in production path).

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. Only 5 role dirs present here (architect, kade, product-manager, silas, wren); engineer/jeff-bridwell-personal-site/shared-observability/wordpress-blog not cloned.

**Action:** Spot-check absent repos on host at standup.

## 6. Stale WIP Cards
**Status: RED**
`roles/kade/current-work.md` last updated 2026-04-18 (~95 days stale). Still shows #2180 (server.ts extraction, 39 handlers done) as active WIP — but shipped cards are now in the #3640–3661 range. `roles/wren/next-session.md` last touched 2026-07-17 (5 days).

**Action:** Kade must refresh `current-work.md` — #2180 status is unknown and board is drifting from file. Wren's file acceptable but should refresh by end of week.

## 7. Domain Context Freshness
**Status: GREEN → WATCH**
All 5 domain-context files (`chorus`, `infrastructure`, `music`, `photos`, `seeds`) last updated 2026-07-17 — 5 days ago, within 7-day threshold. Kade shipped #3657 and #3661 on 2026-07-17 (code domain). No overrun today.

**Action:** If no domain-context update by 2026-07-24, flag red. Chorus and infrastructure domains are highest activity.

## 8. Disk Delta
**Status: N/A**
`data/` contains only `athena/` subdirectory — no perf-baseline snapshots available in this environment. LaunchAgent exists on host only.

**Action:** Run perf-baseline comparison on host machine; no remote data to diff.
