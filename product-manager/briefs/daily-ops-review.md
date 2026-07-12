# Daily Ops Review — 2026-07-12

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile, 0 errors, 0 warnings (29.96s). Same as yesterday.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, +0)**
Grep of `proving/config/launchagents/` confirms ≥16 plists (21 per morning summary) logging to `/tmp/`. No new additions. Card still not filed.
**Action:** Silas — open card to migrate plist stdout/stderr to `~/Library/Logs/Chorus/`.

## 3. CLAUDE.md Fragments
**Status: GREEN (carry)**
PROTOCOL_VERSION 1.4, build 217. 20 shared fragments last touched 2026-07-01 (11d). No divergence between fragment set and root CLAUDE.md.
**Action:** None.

## 4. CSC Compliance
**Status: RED (carry, 3d)**
67 files with `/tmp/` refs in platform scripts (carried from 2026-07-11; path-resolution issue prevented recount today). Role scripts: kade/scripts 5, wren/scripts 2. No new additions confirmed.
**Action:** Silas — reconcile 36→67 count discrepancy; `photo-pipeline.py` highest risk (stateful /tmp); assign July card.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. HEAD: `f029e00 wren: daily morning summary 2026-07-12`.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 96d)**
#1759 "Framework service design — OWL entity model" (Wren, P1) and #1791 "Restore chorus product boundary" (Silas, P1) last updated 2026-04-07 — now 96 days stale with no commits.
**Action:** Wren — close or re-groom both this sprint; >90d WIP is planning debt.

## 7. Domain Context Freshness
**Status: RED (carry, +1d)**
All 5 domain-context files last committed 2026-07-01, now 11d stale (threshold: 7d). Active shipping continues across chorus, photos, infra, seeds, music with no refresh filed.
**Action:** Silas owns chorus/infra; Wren owns music/photos/seeds — overdue; target this sprint.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON surfaced to repo path for delta comparison. Disk size consistent with yesterday (0% growth estimated).
**Action:** Silas — commit nightly baseline JSON to repo path to enable cross-session delta tracking.

---
*Carry items from 2026-07-11: §2 YELLOW, §4/#6/#7 RED. No new regressions today. §4 count discrepancy (36→67) still unresolved — scope must be accurate before filing July card.*
