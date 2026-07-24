# Daily Ops Review — 2026-07-24

## 1. Hooks Health
**Status: YELLOW (2 dead-code warnings, was 0)**
`cargo check` finishes clean (44.63s, 0 errors) but 2 new dead-code warnings: `registration_json` in `session_registry.rs:66` and `owes_response_block` in `nudge_drain.rs:178`. Yesterday: 0 warnings.
**Action:** Silas — suppress or remove dead-code; regression vs yesterday's clean pass.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, 17+2+14)**
17 plists in `proving/config/launchagents/` + 2 in `platform/scripts/launchagents-secondary/` + 14 in `launchagents-canonical/` reference `/tmp/`. proving+secondary count unchanged from Jul 23; canonical 14 not previously tallied.
**Action:** Silas — migration card still open; add canonical-14 to tracking scope.

## 3. CLAUDE.md Fragments
**Status: YELLOW (12d stale, +1d)**
`designing/claudemd/` last committed Jul 12. Now 12d stale, 5d over threshold. Wren was asked to refresh by EOD Jul 23 — no commit found.
**Action:** Wren — claudemd refresh overdue; escalate to Jeff if not landed today.

## 4. CSC Compliance
**Status: RED (carry, 37 sh — unchanged)**
37 `.sh` files in `platform/scripts/` contain `/tmp/` refs. Count unchanged from Jul 23 — no new additions, but no remediation either. Note: #3617 fixed `/tmp` false-fire in hook-server health checks (runtime fix only; scripts still reference `/tmp`).
**Action:** Silas — count static at 37; no regression but no progress. Targeted card still needed.

## 5. Git Dirty State
**Status: GREEN**
`git status` clean — 0 uncommitted changes across all role directories.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 108d, backlog 20d)**
No live Vikunja board access; carrying: #1759/#1791 now 108d without commits (+1d). Wren backlog WIP last touched 2026-07-04 (20d). 5 cards merged today (#3676, #3675, #3672, #3670, #3592) — active velocity, WIP staleness contrast notable.
**Action:** Wren — #1759/#1791 must close or archive; escalate #3607 to Jeff.

## 7. Domain Context Freshness
**Status: RED (9d stale, 5 new cards today)**
All 5 domain-context files last committed Jul 15 (#3613, silas). 5 cards landed Jul 24: #3676 (wren, clearing), #3675 (wren), #3672 (silas), #3670 (silas), #3592 (kade). chorus and infra contexts remain critical. Music/photos/seeds also untouched.
**Action:** Silas/Kade — refresh chorus + infra today; Wren — music/seeds. Silas: quality review also flagged +3 TS errors today (#3617 latency drift 556ms vs 400ms spec).

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON in `data/`; cross-session delta not computable. `platform/scripts/perf-baseline.sh` exists but no output in repo.
**Action:** Silas — land nightly baseline JSON to `data/` to enable tracking.

---
*Carries: §2 YELLOW (17+2+14 plists), §6 RED (108d WIP, backlog 20d). New: §1 YELLOW (2 dead-code warnings). Changes: §3 escalated (12d, Wren missed EOD deadline); §7 RED widens (5 cards today, no context refresh). §4 static at 37.*
