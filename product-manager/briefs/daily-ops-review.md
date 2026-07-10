# Daily Ops Review — 2026-07-10

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — `Finished 'dev' profile` 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17 plists in `proving/config/launchagents/` use `/tmp/` for stdout/stderr. Also `platform/scripts/com.chorus.chorus-ops.plist` hardcodes `/tmp/chorus-ops.log` for StandardOut/Err.
**Action:** Silas — open card to migrate plist stdout/stderr to `~/Library/Logs/Chorus/`.

## 3. CLAUDE.md Fragments
**Status: GREEN (carry)**
PROTOCOL_VERSION 1.4, build 217. 24 shared fragments last committed 2026-07-01 (9d). Manifest pipeline last ran 2026-04-17 (83d).
**Action:** None.

## 4. CSC Compliance
**Status: RED (carry)**
Platform scripts: 36 files with `/tmp/` refs (unchanged). Role scripts: kade/scripts 5 files (`gen-thumbs-bedroom`, `wm-schema-extract`, `photo-pipeline ×2`, `run-canonical-rebuild`); wren/scripts 2 files (`site-walkthrough.mjs`, `style-lint.sh`).
**Action:** Silas — `photo-pipeline.py` highest risk (stateful /tmp); assign July card.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. Latest commits: silas daily quality review today, kade #3632, silas #3629/#3631 all landed cleanly.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 94d)**
#1759 "Framework service design — OWL entity model" (Wren, P1) and #1791 "Restore chorus product boundary" (Silas, P1) both last updated 2026-04-07 — no commits in 94 days.
**Action:** Wren — close or re-groom both this sprint; >90d WIP with no commits is planning debt.

## 7. Domain Context Freshness
**Status: RED (carry)**
All 5 domain-context files stale with active card shipping in their domains: chorus 82d (2026-04-19; #3629 shipped today), photos 106d (kade #3624 shipped 2026-07-07), infrastructure 107d, music 106d, seeds 100d.
**Action:** Silas owns chorus/infra refresh; Wren owns music/photos/seeds — target this sprint.

## 8. Disk Delta
**Status: N/A (carry)**
Repo 661M (platform 331M, roles 80M) — unchanged from yesterday, no growth detected. No runtime baseline JSON for precise delta tracking.
**Action:** Silas — surface nightly baseline JSON to repo path for cross-session delta.

---
*Carry items unchanged from 2026-07-09: #2 (YELLOW), #4/#6/#7 (RED). No new regressions today.*
