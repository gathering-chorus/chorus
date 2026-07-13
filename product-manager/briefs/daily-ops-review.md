# Daily Ops Review — 2026-07-13

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 28.63s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, +0)**
17 plists in `proving/config/launchagents/` log to `/tmp/`. Count stable vs yesterday. No card filed.
**Action:** Silas — open card to migrate plist stdout/stderr to `~/Library/Logs/Chorus/`.

## 3. CLAUDE.md Fragments
**Status: YELLOW (was GREEN)**
24 shared fragments under `designing/claudemd/shared/` all last touched 10 days ago (2026-07-03). Crossed 7d threshold; no divergence from root CLAUDE.md confirmed, but protocol currency is at risk.
**Action:** Wren — audit fragments for drift vs recent card activity; refresh any referencing stale behavior.

## 4. CSC Compliance
**Status: RED (carry, 4d)**
38 unique script files under `platform/scripts/` reference `/tmp/` (dedup applied; 36→67→38 count now stabilized). `tmp-reaper.sh`, `look.sh`, `crawler-hydrate-graph.sh` highest risk. No new additions today.
**Action:** Silas — 38 is the canonical count; file July card with migration scope.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. HEAD: `dcd4e93 silas: daily quality review 2026-07-13`.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 97d)**
#1759 "OWL entity model" (Wren, P1) and #1791 "Restore chorus product boundary" (Silas, P1) last updated 2026-04-07 — 97 days stale. No commits for either in 7-day git log.
**Action:** Wren — close or re-groom both; >90d WIP is planning debt blocking sprint clarity.

## 7. Domain Context Freshness
**Status: RED (carry, +1d)**
All 5 domain-context files (chorus, infra, music, photos, seeds) last committed ~10 days ago. 20+ cards shipped this week across chorus/infra/seeds — context lagging active ship rate by 3d+.
**Action:** Silas owns chorus/infra; Wren owns music/photos/seeds — both overdue.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON committed to repo; cross-session delta not computable.
**Action:** Silas — commit nightly baseline JSON to enable delta tracking.

---
*Carries: §2 YELLOW, §4/#6/#7 RED. New: §3 bumped to YELLOW (fragment age crossed 7d). CSC count stabilized at 38 (dedup fix resolves 36→67 discrepancy).*
