# Daily Ops Review — 2026-07-15

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 28.09s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (recount: 17, was 18)**
17 plist files in `proving/config/launchagents/` reference `/tmp/`. Corrected from yesterday's 18 — `config/launchagents/com.chorus.tmp-reaper.plist` was a double-count. No new additions.
**Action:** Silas — 17 is today's canonical count; migration card to `~/Library/Logs/Chorus/` still open.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry, 12d)**
All `designing/claudemd/` fragments last committed 2026-07-03; 12d stale (+1d). No refreshes despite continued card shipping.
**Action:** Wren — audit for drift; now past 10d, escalating.

## 4. CSC Compliance
**Status: RED (recount: 36 sh-only)**
36 `.sh` files in `platform/scripts/` contain `/tmp/` refs. Yesterday's 67 included non-.sh files and secondary dirs — sh-only canonical count: 36. Core offenders unchanged: `look.sh`, `bridge-subscriber-watchdog.sh`, `werk-init.sh`, `crawler-hydrate-graph.sh`.
**Action:** Silas — adopt 36 as canonical sh-only baseline; open July card scoped to `platform/scripts/*.sh`.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. HEAD: `150f865` (silas: daily quality review 2026-07-15).
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 99d)**
#1759 "OWL entity model" (Wren, P1) and #1791 "Restore chorus product boundary" (Silas, P1) last touched 2026-04-07 — now 99d stale. No commit activity on either this week.
**Action:** Wren — close or re-groom both; 100d mark tomorrow.

## 7. Domain Context Freshness
**Status: YELLOW (partial recovery)**
`domain-context-photos.md` updated today via #3599 — GREEN. Chorus, infra, music, seeds last committed 2026-07-03 (12d); chorus domain shipped 4 cards this week (#3641, #3643, #3646, #3647).
**Action:** Silas — chorus/infra context most urgent; Wren — music/seeds at 12d.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON committed to repo; cross-session delta not computable.
**Action:** Silas — land nightly baseline JSON to `data/` to enable tracking.

---
*New: §2 recount −1 (now 17); §4 recount sh-only (now 36); §7 photos GREEN. Carries: §3 YELLOW (12d), §4 RED (sh-36), §6 RED (99d), §7 YELLOW (chorus/infra 12d). §1/§5 green.*
