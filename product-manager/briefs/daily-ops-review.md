# Daily Ops Review — 2026-03-29

> Feeds Wren's morning summary. Each section: status · finding · action.

---

## 1. Hooks Health
**🟡 YELLOW** — `cargo check` clean, 8 warnings (4 auto-fixable, 4 manual). `decision_block_json` unused fn flagged.
**Action:** Run `cargo fix --bin chorus-hooks` to apply 4 suggestions; review remaining 4 manually this week.

## 2. LaunchAgent /tmp Refs
**🟡 YELLOW** — 6 plists use `/tmp/` for logs: chorus-ops, andon-enrich, andon-light (messages/scripts); alertmanager, app, blackbox-exporter + 7 others (architect/scripts/launchagents).
**Action:** Known pattern. Confirm log rotation exists for alertmanager/app; /tmp log loss on reboot is accepted risk per prior review.

## 3. CLAUDE.md Conflicts
**🟢 GREEN** — Role fragments differ intentionally by design. `close-out-docs.md`: Wren has full doc checklist; Silas defers to Rust `health-daily` gate (by role contract). No staleness detected.
**Action:** None.

## 4. CSC Compliance (/tmp in scripts)
**🟡 YELLOW** — 7 scripts use `/tmp/`: bedroom-heartbeat.sh (state+log), werk-init.sh (cache), bridge-subscriber.js (inbox), jeff-input-monitor.swift (scan dir), chorus-query.sh, andon-light.swift, chorus-ops.sh (lock+cache).
**Action:** Confirm `/tmp/claude-team-scan/` is cleaned on session-init. bedroom-heartbeat state file lost on reboot — card if not acceptable.

## 5. Git Dirty State
**🟡 YELLOW** — `messages/` shows 321 untracked files, all in `chorus-hooks/target/` (build artifacts from today's cargo check). All other 6 role dirs clean.
**Action:** Confirm `messages/services/chorus-hooks/target/` is in `.gitignore`; add if not.

## 6. Stale WIP Cards (>48h)
**🟢 GREEN** — 2 open WIP cards: `#1794` (Kade — Fuseki seed migration, awaiting `/acp`) and `#1783` (Wren — ontology OWL, needs Integration class + Fuseki load). Both ~24h old, under threshold.
**Action:** Watch #1783 — complex card; flag stale if not accepted by EOD 2026-03-30.

## 7. Domain Context Freshness
**🟢 GREEN** — All 4 domain-context files current: chorus (today), infrastructure/music/photos (~24h, 2026-03-28). Recent Chorus cards (#1795–1797) aligned with same-day context update.
**Action:** None.

## 8. Disk Delta
**🟢 GREEN** — Mar 26 → Mar 27: 645.5 GB → 650.9 GB (+5.0 GB, **+0.84%**). Under 2% threshold.
**Action:** None. Note: baseline JSON has malformed `"lastHour": 00` — fix baseline script to emit valid JSON integers.
