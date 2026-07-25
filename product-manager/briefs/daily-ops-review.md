# Daily Ops Review — 2026-07-25

## 1. Hooks Health
**Status: YELLOW (carry, 2 dead-code warnings)**
`cargo check` passes clean (0 errors). Same 2 dead-code warnings as Jul 24: `owes_response_block` and `registration_json` (each fires on both `chorus-hooks` + `chorus-hook-shim` binaries = 4 warning lines). No regression, no remediation.
**Action:** Silas — suppress or remove dead-code; carry from yesterday.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, 17+2 plists)**
17 plists in `proving/config/launchagents/` + 2 in `platform/scripts/launchagents-secondary/` reference `/tmp/`. Count unchanged from Jul 24.
**Action:** Silas — migration card still open; no movement.

## 3. CLAUDE.md Fragments
**Status: YELLOW (13d stale, +1d)**
`designing/claudemd/` last committed Jul 12. Now 13d stale, 6d over threshold. Wren missed EOD Jul 23 deadline; no refresh landed Jul 24 either.
**Action:** Wren — claudemd refresh now 2 days overdue; escalate to Jeff today.

## 4. CSC Compliance
**Status: RED (carry, 37 .sh files — unchanged)**
37 `.sh` files in `platform/scripts/` contain `/tmp/` refs (51 total including .py/.js). Count static vs Jul 24 — no regressions, no progress.
**Action:** Silas — no movement; targeted remediation card still needed.

## 5. Git Dirty State
**Status: GREEN**
`git status` clean — 0 uncommitted changes. HEAD detached on main as expected in remote session. 9 cards merged Jul 25.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, board snapshots 109d stale)**
Board snapshots in `platform/logs/` dated 2026-04-07 (109d old) — live WIP not assessable. Carrying: #1759/#1791 at 109d without commits (+1d). 9 cards merged today, active velocity continues.
**Action:** Wren — #1759/#1791 must close or archive; board snapshot refresh needed for accurate WIP count.

## 7. Domain Context Freshness
**Status: RED (all 5 files, 97–122d stale)**
- `chorus`: 97d (2026-04-19, Silas #2234)
- `seeds`: 115d (2026-04-01, Wren #1942)
- `infrastructure`: 122d (2026-03-25, Silas #1688)
- `music` / `photos`: 121d (2026-03-26, Wren #1688)

9 cards landed Jul 25 span chorus, infra, app, and clearing domains — context files still untouched after yesterday's ask.
**Action:** Silas — chorus + infra critical; Wren — seeds/music/photos. All contexts dramatically past threshold.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` output present in repo or `proving/logs/`. Cross-session delta not computable.
**Action:** Silas — land nightly baseline JSON to `proving/logs/` to enable tracking.

---
*Carries: §1 YELLOW (2 dead-code warnings), §2 YELLOW (17+2 plists), §4 RED (37 sh), §6 RED (109d WIP). Escalations: §3 +1d (13d stale, 2nd missed deadline — escalate to Jeff); §7 unchanged RED (97–122d, 9 more cards landed today).*
