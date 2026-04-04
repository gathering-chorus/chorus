# Daily Ops Review — 2026-04-04

> Feeds Wren's morning summary. Each section: status · finding · action.

---

## 1. Hooks Health
**YELLOW** — `cargo check` passes, 30 warnings (up from 18 on 04-02). 9 auto-fixable. Dead code: `decision_block_json` at `src/types.rs:175` still unresolved.
**Action:** Run `cargo fix --bin "chorus-hooks" -p chorus-hooks`; wire or remove `decision_block_json`.

## 2. LaunchAgent /tmp Refs
**YELLOW** — 13+ plists route stdout/stderr to `/tmp/` (alert-notifier, api, clearing, context-cache ×3, cruft-scan, fuseki ×2, others). Pattern systemic and pre-existing.
**Action:** Migrate log paths to `~/Library/Logs/chorus/` — `/tmp` is volatile and purged on reboot.

## 3. CLAUDE.md Conflicts
**GREEN** — All 6 role CLAUDE.md files updated 2026-04-03 or 2026-04-04. `messages/claudemd/` fragment dir in ops spec does not exist — spec path is stale.
**Action:** Update ops spec to reference `chorus/*/CLAUDE.md`; no content conflicts detected.

## 4. CSC Compliance
**YELLOW** — 15+ `/tmp/` refs in `chorus/platform/scripts/`: `look.sh`, `werk-init.sh`, `bridge-subscriber.js`, `bedroom-heartbeat.sh`, `chorus-ops.sh`, `chorus-query.sh`, `listen.sh`, others.
**Action:** Audit each for boot-persistence risk; replace with `$TMPDIR` or `~/Library/Caches/chorus/`.

## 5. Git Dirty State
**GREEN** — Repo fully clean. `git status --short` returns empty.
**Action:** None.

## 6. Stale WIP Cards (>48h)
**RED** — 4 WIP cards last touched 2026-03-29 (6 days stale, >48h threshold): "Unify Chorus repo", "Clearing UI validation tests", "Wire express-prom-bundle for request metrics", "Add structured logging to messaging tier".
**Action:** Wren to triage — assign owner or move to backlog before next session.

## 7. Domain Context Freshness
**GREEN** — All 5 domain-context files updated 2026-04-03 or 2026-04-04. No stale contexts.
**Action:** None.

## 8. Disk Delta
**RED** — Week-over-week growth: 432 GB → 957 GB (+121% in 7 days). `percentUsed` field in perf-baseline JSON is corrupt (shows 2/3%; actual ~23%/~52% of 1.86 TB). Also: `errors.lastHour` bare `00` breaks JSON parser.
**Action:** Immediate — identify what grew ~525 GB (suspect Fuseki, logs, model artifacts). Fix perf-baseline.sh JSON encoding bug.
