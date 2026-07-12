# Daily Ops Review — 2026-07-11

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — `Finished 'dev' profile` 0 errors, 0 warnings (32s).
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17 plists in `proving/config/launchagents/` + 4 in `platform/scripts/launchagents-secondary/` log to `/tmp/`. No change from yesterday.
**Action:** Silas — open card to migrate plist stdout/stderr to `~/Library/Logs/Chorus/`.

## 3. CLAUDE.md Fragments
**Status: GREEN (carry)**
PROTOCOL_VERSION 1.4, build 217. 20 shared fragments last committed 2026-07-01 (10d). Root CLAUDE.md last updated 2026-07-08 (3d). No divergence detected.
**Action:** None.

## 4. CSC Compliance
**Status: RED (carry)**
Platform scripts: 67 files with `/tmp/` refs (yesterday reported 36 — discrepancy may reflect wider grep scope; investigate). Role scripts: kade/scripts 5 files, wren/scripts 2 files. No new additions confirmed.
**Action:** Silas — reconcile count discrepancy; `photo-pipeline.py` highest risk (stateful /tmp); assign July card.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. Latest commits: silas daily quality review 2026-07-11, kade #3632, silas #3629/#3631 landed cleanly.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 95d)**
#1759 "Framework service design — OWL entity model" (Wren, P1) and #1791 "Restore chorus product boundary" (Silas, P1) last updated 2026-04-07 — 95 days with no commits. GitHub search unavailable this session; status from prior review.
**Action:** Wren — close or re-groom both this sprint; >90d WIP is planning debt.

## 7. Domain Context Freshness
**Status: RED (carry, +1d)**
All 5 domain-context files last committed 2026-07-01 (10d stale). Active cards shipped this week in all 5 domains: chorus (#3629, #3631, #3619, #3625, #3618), photos (#3624, #3621, #3632), infra (#3431), seeds (#2588), music (#3622 door work adjacent).
**Action:** Silas owns chorus/infra refresh; Wren owns music/photos/seeds — overdue; target this sprint.

## 8. Disk Delta
**Status: N/A (carry)**
Repo 661M (platform 331M, roles 80M) — identical to yesterday, 0% growth. No perf-baseline JSON available for precise delta.
**Action:** Silas — surface nightly baseline JSON to repo path for cross-session delta tracking.

---
*Carry items from 2026-07-10: #2 (YELLOW), #4/#6/#7 (RED). No new regressions. New flag: §4 count discrepancy 36→67 warrants investigation.*
