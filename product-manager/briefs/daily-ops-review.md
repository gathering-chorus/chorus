# Daily Ops Review — 2026-07-26

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes clean. Same 2 dead-code warnings as Jul 25: `owes_response_block` (nudge_drain.rs:178) and `registration_json` (session_registry.rs:66), each on both binaries = 4 warning lines. No regression.
**Action:** Silas — suppress or remove dead-code; carry from yesterday.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17 plists in `proving/config/launchagents/` + 2 in `platform/scripts/launchagents-secondary/` reference `/tmp/`. Count unchanged from Jul 25.
**Action:** Silas — migration card still open; no movement.

## 3. CLAUDE.md Fragments
**Status: YELLOW (partial improvement)**
kade #3288 (Jul 25 09:04) updated `PROTOCOL_VERSION`, `shared/chorus-prompt.md`, and `version-ledger.json`. Role-specific fragments (roles/wren, roles/silas, roles/kade) still not refreshed — last touched prior to Jul 25.
**Action:** Wren + Kade — role fragments still need content refresh; shared components now current.

## 4. CSC Compliance
**Status: RED (carry) / N/A (specified paths)**
`messages/scripts/` and `architect/scripts/` don't exist in this repo. `platform/scripts/` has **37 .sh files** with `/tmp/` refs — count unchanged from Jul 25; no remediation.
**Action:** Silas — targeted remediation card still needed; CSC check paths mismatch is a separate tracking gap.

## 5. Git Dirty State
**Status: GREEN**
`git status` clean — 0 uncommitted changes. 5 role commits landed Jul 25–26 (silas #3695, wren #3696/#3674, kade #3392/#3692).
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry)**
Board snapshots 110d stale; Vikunja not queryable from session. From git: #3695/#3696/#3392/#3692 all closed this cycle — active velocity. #1759/#1791 still carry with no commit activity (now 110d+).
**Action:** Wren — #1759/#1791 must close or archive; board snapshot refresh needed.

## 7. Domain Context Freshness
**Status: RED (carry, 98–123d stale)**
All 5 context files unchanged. Jul 25–26 shipped 4 cards spanning chorus and infrastructure domains:
- `domain-context-chorus.md`: 98d stale — 2 Chorus cards landed (#3696 Buzz leg A, #3695 key binding)
- `domain-context-infrastructure.md`: 123d stale — #3695 also infra-touching
**Action:** Silas — chorus + infra critical; Wren — seeds/music/photos. All past threshold.

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` output in repo or `proving/logs/`. Scripts present but produce no captured output.
**Action:** Silas — land nightly baseline JSON to `proving/logs/` to enable delta tracking.

---
*Carries: §1 YELLOW (2 dead-code), §2 YELLOW (17+2 plists), §4 RED (37 sh), §6 RED (110d WIP snapshots), §8 N/A. New: §3 improved (shared components updated Jul 25 via #3288, role fragments still pending). Escalation: §7 RED +2 chorus cards shipped into stale context.*
