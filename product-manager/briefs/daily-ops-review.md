# Daily Ops Review — 2026-07-14

## 1. Hooks Health
**Status: GREEN**
`cargo check` on `platform/services/chorus-hooks` passes clean — Finished dev profile in 29.46s, 0 errors, 0 warnings.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry, recount +1 → 18)**
18 plists reference `/tmp/` (17 in `proving/config/launchagents/`, 1 in `config/launchagents/com.chorus.tmp-reaper.plist` previously undercounted). No new additions.
**Action:** Silas — 18 is the corrected canonical count; open card to migrate all plist stdout/stderr to `~/Library/Logs/Chorus/`.

## 3. CLAUDE.md Fragments
**Status: YELLOW (carry, 11d)**
All `designing/claudemd/` and `designing/domain-context/` files last committed 2026-07-03 (wren: #3603); now 11 days stale. 30+ cards shipped since; no fragment refresh.
**Action:** Wren — audit shared fragments for drift vs recent card activity and refresh.

## 4. CSC Compliance
**Status: RED (carry — count discrepancy flagged)**
`grep -rl '/tmp/' platform/scripts/` returns 67 unique files today; yesterday's brief states canonical=38. Difference likely includes `.plist` files in `platform/scripts/launchagents-secondary/`. Key script offenders unchanged: `coherence-check`, `look.sh`, `werk-init.sh`, `bridge-subscriber.js`, `crawler-hydrate-graph.sh`.
**Action:** Silas — re-establish canonical count (scripts-only vs all files); open July card with scope.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. HEAD: `53b60dc #3639 (kade) (#760)`.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 98d)**
#1759 "OWL entity model" (Wren, P1) and #1791 "Restore chorus product boundary" (Silas, P1) last updated 2026-04-07 — now 98 days stale. No commits for either in 7-day git log.
**Action:** Wren — close or re-groom both; >90d WIP is planning debt blocking sprint clarity.

## 7. Domain Context Freshness
**Status: RED (carry, 11d)**
All 5 domain-context files (chorus, infra, music, photos, seeds) last committed 2026-07-03. Music/photos domain active this week: #3624 (kade, sexuality-player). Chorus also shipping (#3635, #3632, #3629). Context lag now 4d past the 7d threshold.
**Action:** Silas owns chorus/infra; Wren owns music/photos/seeds — both overdue.

## 8. Disk Delta
**Status: N/A (carry)**
No perf-baseline JSON committed to repo; cross-session delta not computable.
**Action:** Silas — commit nightly baseline JSON to enable delta tracking.

---
*Carries: §2 YELLOW (+1 recount), §3 YELLOW (11d), §4 RED (count dispute flagged), §6 RED (98d), §7 RED (11d). No new issues. §1/§5 green.*
