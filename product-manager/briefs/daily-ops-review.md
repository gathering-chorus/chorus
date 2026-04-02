# Daily Ops Review — 2026-04-02

> Feeds Wren's morning summary. Each section: status · finding · action.

---

## 1. Hooks Health
**YELLOW** — `cargo check` passes (no errors), 18 warnings. 4 auto-fixable; `decision_block_json` unused fn at `src/types.rs:175`.
**Action:** Run `cargo fix --bin "chorus-hooks" -p chorus-hooks`; review remaining warnings this sprint.

## 2. LaunchAgent /tmp Refs
**YELLOW** — 36 plist files reference `/tmp/` for stdout/stderr logs across `proving/` and `platform/` launchagents. Pattern is systemic and pre-existing.
**Action:** Confirm log rotation covers key agents; document as accepted risk if /tmp log loss on reboot is OK per prior review.

## 3. CLAUDE.md Conflicts
**YELLOW** — `messages/claudemd/` fragment directory does not exist at the expected path. All 7 role `CLAUDE.md` files last touched 2026-04-01 — fresh individually, but no fragment diffing is possible.
**Action:** Clarify whether claudemd fragment system was deprecated or path moved; update ops check accordingly.

## 4. CSC Compliance
**GREEN** — No `/tmp/` hits in `chorus/messages/scripts/` or `chorus/architect/scripts/`. Clean.
**Action:** None.

## 5. Git Dirty State
**GREEN** — Repo fully clean. `git status --short` returns empty.
**Action:** None.

## 6. Stale WIP Cards (>48h)
**RED** — Card `#1926` (Silas — gate integration test suite, 39/39 passing, awaiting `/acp`) last updated 2026-03-31 18:24. Age: ~54h, over threshold. Card `#1865` (photo thumbnail) also in WIP but not started.
**Action:** Issue `/acp` for #1926 or explicitly defer. Move #1865 back to Queue — it has no active work.

## 7. Domain Context Freshness
**GREEN** — All 5 domain-context files updated 2026-04-01 or 2026-04-02. Latest shipped cards (#1956 domain crawler, #1957 awareness diagrams) align with seeds/chorus context updates.
**Action:** None.

## 8. Disk Delta
**YELLOW** — Baseline data anomaly: `usedBytes` decreased 464 GB → 257 GB (Mar 28→29) while `percentUsed` increased 3%→5%. Values are mutually inconsistent — likely a reporting bug in `perf-baseline.sh`.
**Action:** Fix baseline script to emit consistent `usedBytes`/`percentUsed`; delta comparison is unreliable until corrected.
