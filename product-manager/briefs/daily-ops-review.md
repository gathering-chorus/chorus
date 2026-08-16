# Daily Ops Review — 2026-08-16

_Run at UTC. Feeds Wren's morning summary._

---

## 1. Hooks Health
**🟡 YELLOW** — `cargo check` passes; warning count climbed from 4 → 7 (day 14, deadline blown)
- Dead-code set: `clear_cap_cache`, `Liveness` enum, `probe_role_session` + 4 more
- **Action:** Wren/Silas — EOD deadline was today; remove dead-code set or file a card now

---

## 2. LaunchAgent /tmp Refs
**🟡 YELLOW** — 17 plists carry `/tmp` log paths; all stdout/stderr only (unchanged, day 15+)
- Files: alert-notifier, api, clearing, context-cache-{daily,hourly,weekly}, cruft-scan, fuseki-{compact,perf}, harvest-exporter, hooks, jeff-input-monitor, launchagent-metrics, nudge-health, ops, perf-baseline, posture-capture
- **Action:** Silas to open migration card (target `$TMPDIR` / `~/.chorus/logs/`); no card yet

---

## 3. CLAUDE.md Fragment Staleness
**🔴 RED** — Deadline missed; no claudemd commits in 7+ days; PROTOCOL_VERSION still 1.6.0
- Wren was assigned EOD Aug 16 bump to v1.6.1+; not done as of this run
- **Action:** Wren: bump fragment + CLAUDE.md today; this is overdue

---

## 4. CSC Compliance (/tmp in Scripts)
**🔴 RED** — 70 platform/scripts files + 2 wren/scripts + 4 kade/scripts carry `/tmp/` refs
- No migration card, no movement from yesterday; floor not moving
- **Action:** Silas to file migration card; Wren owns role-script refs

---

## 5. Git Dirty State
**🟢 GREEN** — Working tree clean; 41 commits landed since Aug 14, all pushed

---

## 6. Stale WIP Cards
**🔴 RED** — Card #1962 still in `building` since 2026-04-12 (126 days); state.json unchanged
- Wren was asked to close this yesterday; action not taken
- **Action:** Wren to close #1962 immediately — zombie card is blocking state clarity

---

## 7. Domain Context Freshness
**🔴 RED** — Day 4 breach; all 5 domain-context files touched 2026-08-14 (code commit, not content)
- domain-context-chorus.md and domain-context-infrastructure.md have months-stale content; 10+ cards shipped since last refresh
- **Action:** Wren: content update required today; domain-context-seeds.md also suspect

---

## 8. Disk / Perf Baseline Delta
**⚪ N/A** — No baseline snapshot data in repo; perf-baseline scripts present but no output committed
- **Action:** Silas to verify nightly LaunchAgent is firing and route output to repo or spine

---

_Total: 4 RED, 2 YELLOW, 1 GREEN, 1 N/A (escalation from yesterday: CLAUDE.md deadline blown, warning count up)_
