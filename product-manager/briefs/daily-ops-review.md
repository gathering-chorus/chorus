# Daily Ops Review — 2026-07-09

## 1. Hooks Health
**Status: GREEN (resolved)**
`cargo check` passes with 0 warnings — dead code symbols from yesterday's YELLOW carry (8 warnings: `load_role_sections`, `find_most_recent_pending`, etc.) are gone. Clean build.
**Action:** None.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW (carry)**
17 distinct plist files in `proving/config/launchagents/` use `/tmp/` for stdout/stderr (hooks, api, clearing, context-cache × 3, fuseki, ops, posture-capture, etc.). Structural, unchanged.
**Action:** Silas — open card to migrate StandardOut/Err to `~/Library/Logs/Chorus/`.

## 3. CLAUDE.md Fragments
**Status: GREEN (carry)**
24 shared fragments in `designing/claudemd/shared/` current; manifest build 217 / PROTOCOL_VERSION 1.4. Last pipeline run 2026-02-21 — see #8 risk note.
**Action:** None.

## 4. CSC Compliance
**Status: RED (carry)**
Platform scripts clean (no new /tmp refs). Role scripts: kade/scripts/ has 5 hardcoded `/tmp/` paths (gen-thumbs-bedroom, wm-schema-extract, photo-pipeline ×2, run-canonical-rebuild); wren/scripts/ has 2 (site-walkthrough.mjs, style-lint.sh).
**Action:** Silas — `photo-pipeline.py` /tmp state is highest risk; assign July card.

## 5. Git Dirty State
**Status: GREEN**
Repo clean — 0 uncommitted changes. Recent commits: silas daily quality review 2026-07-09, kade #2588/#3431, silas #3619 all landed cleanly.
**Action:** None.

## 6. Stale WIP Cards
**Status: RED (carry, 93d)**
2 WIP cards both last updated 2026-04-07 (93 days): #1759 "Framework service design — OWL entity model unifying borg chorus and jb ontologies" (Wren, P1) and #1791 "Restore chorus product boundary — chorus/ as namespace within platform/" (Silas, P1).
**Action:** Wren — close or re-groom both; >90 days in WIP with no commits is planning debt.

## 7. Domain Context Freshness
**Status: RED**
All 5 domain-context files stale by content date: chorus 2026-04-19 (81d, 9+ cards shipped), infrastructure 2026-03-25 (106d), music 2026-03-26 (105d), photos 2026-03-26 (105d), seeds 2026-04-01 (99d). File mtimes from clone (2026-07-08) masked this; yesterday's GREEN was a false read.
**Action:** Assign refresh sweep — Silas owns chorus/infra, Wren owns music/photos/seeds; target this sprint.

## 8. Disk Delta
**Status: N/A (carry)**
No runtime perf-baseline snapshots in repo. Repo total: 661M (platform 331M, roles 80M).
**Action:** Silas — surface nightly baseline JSON to repo path for cross-session delta tracking.
