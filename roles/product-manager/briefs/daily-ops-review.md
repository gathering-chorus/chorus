# Daily Ops Review — 2026-09-05

## 1. Hooks Health
**Status: RED** (day 7)
`cargo check` fails with 2 errors in `signal_witness.rs:55`: `si_pid` / `si_uid` accessed as struct fields but are methods in this libc version. Fix: `(*info).si_pid()` and `(*info).si_uid()`. Binary not buildable since 2026-08-30.
**Action:** Silas or Wren — one-line fix, open a card if not already queued.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW** (persistent)
12+ plists in `proving/config/launchagents/` use `/tmp/` for stdout/stderr log paths (hooks, api, clearing, context-cache ×3, fuseki ×2, harvest-exporter, posture-capture, ops, seed-probe). No remediation shipped.
**Action:** CSC hygiene card still open. Redirect log paths to `~/Library/Logs/Chorus/`.

## 3. CLAUDE.md Fragment Staleness
**Status: GREEN** (resolved)
All 24 fragments in `designing/claudemd/shared/` updated 2026-09-05 today. Prior RED resolved.
**Action:** None.

## 4. CSC Compliance (/tmp in scripts)
**Status: YELLOW** (persistent)
`platform/scripts/` has 14+ `/tmp/` refs (`look.sh`, `bridge-subscriber-watchdog.sh`, `nightly-suites.sh`, `athena-deploy-model.sh` ×9+). `roles/wren/scripts/style-lint.sh` uses `/tmp/style-lint-body.html`. Ephemeral use; not a blocking issue.
**Action:** Known ongoing. LaunchAgent log paths remain the priority (§2).

## 5. Git Dirty State
**Status: GREEN**
Working tree clean — 0 uncommitted changes across all role directories.
**Action:** None.

## 6. Stale WIP Cards
**Status: YELLOW**
chorus-api MCP offline (day 7), live board unqueryable. Wren's `state.json` shows stale `card:1962` timestamp (April). Recent git log confirms active shipping: Wren #4101 (22h), #4096 (2d); Kade #4105, #4106 (9–18h); Silas #4107, #4108 (9–13h). Wren state.json not updated post-land.
**Action:** Wren update `state.json` after each land. Silas restore chorus-api to restore board visibility.

## 7. Domain Context Freshness
**Status: GREEN** (resolved)
All 5 domain-context files (`chorus`, `infrastructure`, `music`, `photos`, `seeds`) updated 2026-09-05 today. Prior RED resolved.
**Action:** None.

## 8. Disk Delta
**Status: N/A**
No perf-baseline output data in repo. Cannot compute disk growth delta.
**Action:** No change from prior reviews. Baseline artifact not yet established.
