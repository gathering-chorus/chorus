# Daily Ops Review — 2026-05-22

## 1. Hooks Health
**Status: YELLOW**
`cargo check` compiles (41s, dev profile) but grep finds 252 raw warning lines — up from 9 last week. Large dead-code cluster around a partial approval workflow: `handle_approval_request`, `sweep_stale_pending`, `build_cards_add_argv`, `ApprovalSignal`, `PendingPayload`, and ~14 related symbols never used. `chorus_worktree_override` and `load_role_sections` persist from last week.

**Action:** File cleanup card for approval-workflow dead code; either complete the feature or remove the scaffolding.

## 2. LaunchAgent /tmp Refs
**Status: YELLOW** *(unresolved from 05-15)*
Same two offending plists as last week: `com.chorus.hooks.plist` logs to `/tmp/chorus-hooks.{stdout,stderr}.log`; `com.chorus.chorus-ops.plist` logs to `/tmp/chorus-ops.log`. tmp-reaper itself is expected.

**Action:** Redirect daemon logs to `~/Library/Logs/Chorus/`; this is the second consecutive week at YELLOW.

## 3. CLAUDE.md Fragment Staleness
**Status: GREEN**
`messages/claudemd/` path does not exist in this environment; `designing/claudemd/` fragment tree intact per 05-15 review. No regression detected.

**Action:** None.

## 4. CSC Compliance (/tmp/ in Scripts)
**Status: GREEN**
No `/tmp/` refs in `messages/scripts/` or `architect/scripts/` — both directories absent from this repo clone, check vacuously passes. Platform-level violations tracked separately.

**Action:** None for scoped paths.

## 5. Git Dirty State
**Status: GREEN**
`gathering-team` repo is clean. Seven expected role directories (`product-manager`, `architect`, `engineer`, `messages`, etc.) are not top-level in this clone — they live under `roles/`; no dirty state there either.

**Action:** None.

## 6. Stale WIP Cards
**Status: RED**
Board snapshots are 45 days old (last captured 2026-04-07). Two cards stuck in WIP with no update since snapshot date:
- "Framework service design — OWL entity model unifying borg chorus and jb ontologies"
- "Restore chorus product boundary — chorus/ as namespace within platform/"

**Action:** Refresh board snapshots immediately; confirm both WIP cards are actively assigned or park them. Silas to run `chorus-board-snapshot` on next session start.

## 7. Domain Context Freshness
**Status: YELLOW**
`domain-context-infrastructure.md` is fresh (updated today). Four others are 6 days old — one day from threshold: seeds, photos, music, and chorus. Card #3029 shipped in `domain:chorus` this week.

**Action:** Silas or Wren to refresh `domain-context-chorus.md` today before it breaches 7 days.

## 8. Disk Delta
**Status: N/A**
No runtime perf-baseline data in this environment. Repo total: 341MB. No prior snapshot to diff against.

**Action:** Run perf-baseline comparison locally; no remote data to diff here.
