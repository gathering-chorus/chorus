# Daily Ops Review — 2026-08-01

## 1. Hooks Health
**Status: YELLOW (carry)**
`cargo check` passes — 2 dead-code warnings persist: `registration_json` (session_registry.rs:66), `owes_response_block` (nudge_drain.rs:178). No errors, no regression vs Jul 31.
**Action:** Silas — suppress or remove both dead-code paths.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
33 `/tmp/` refs across 17 plist files in `proving/config/launchagents/`. Count unchanged from Jul 30 and Jul 31.
**Action:** Silas — migration card open; no movement.

## 3. CLAUDE.md Fragments
**Status: RED (day 2)**
`designing/claudemd/` fragments last committed 2026-07-25 — now 8d stale. No update in today's commits (#3717, #3689, quality review). Prompt path `messages/claudemd/` is wrong; actual path is `designing/claudemd/`.
**Action:** Wren + Kade — refresh role content; fix ops prompt path.

## 4. CSC Compliance
**Status: RED (carry)**
`platform/scripts/` has **149 `/tmp/`** occurrences (unchanged from Jul 30–31). Prompt paths `messages/scripts/` and `architect/scripts/` do not exist.
**Action:** Silas — count held, no new regressions. Update prompt to scan `platform/scripts/`.

## 5. Git Dirty State
**Status: GREEN**
0 uncommitted changes. All committed and clean.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry)**
Board snapshot is from 2026-04-07 (stale). 2 visible WIP cards: #1759 (Wren, >117d) and #1791 (Silas, >117d). GitHub Dependabot PRs #449 and #443 at **59d open**.
**Action:** Jeff/Silas — close or merge Dependabot PRs; refresh board snapshot.

## 7. Domain Context Freshness
**Status: RED (day 2)**
All 5 domain-context files last committed 2026-07-25 (8d stale). Cards #3717 (Kade) and #3689 (Silas) landed today with no domain context refresh. Chorus context last substantively updated 2026-04-19 (104d).
**Action:** Silas — update `domain-context-chorus.md` now (overdue 104d).

## 8. Disk Delta
**Status: N/A (carry)**
No `perf-baseline-*.json` in repo. Script targets macOS `diskutil`, not runnable in remote env.
**Action:** Silas — land nightly baseline JSON to enable delta tracking.

---
*Aug 1 delta: §3 fragments and §7 domain context both now 8d stale (no refresh despite 2 cards landing today). §6 Dependabot PRs tick to 59d. §1 hooks check now confirmed runnable in this env — YELLOW (warnings only, not errors). All /tmp counts held. Board snapshot stale (Apr 2026) — board state unverifiable.*
