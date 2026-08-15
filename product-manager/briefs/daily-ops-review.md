# Daily Ops Review — 2026-08-15

_Run at 10:15 UTC. Feeds Wren's morning summary._

---

## 1. Hooks Health
**🟡 YELLOW** — `cargo check` clean, 4 dead-code warnings (day 13 of accumulation)
- `clear_cap_cache`, `Liveness` enum, `probe_role_session` + 1 unreferenced fn in word_cap.rs
- **Action:** Address dead-code set before day 14 EOD (Wren/Silas). No blockers, no errors.

---

## 2. LaunchAgent /tmp Refs
**🟡 YELLOW** — 17 plists carry `/tmp` log paths; all are stdout/stderr only (not runtime state paths)
- Files: alert-notifier, api, clearing, context-cache-{daily,hourly,weekly}, cruft-scan, fuseki-{compact,perf,compact}, harvest-exporter, hooks, jeff-input-monitor, launchagent-metrics, nudge-health, ops, perf-baseline, posture-capture
- Status unchanged >14d; no migration card visible
- **Action:** Silas to open migration card and assign; target `$TMPDIR` or `~/.chorus/logs/`

---

## 3. CLAUDE.md Fragment Staleness
**🟡 YELLOW** — Fragments (roles/wren, roles/silas, roles/kade) last updated 3 days ago (#3831); root CLAUDE.md refresh due **Aug 16 (tomorrow)**
- manifest.json at `_build: 217`; PROTOCOL_VERSION is single source for human version string
- **Action:** Wren must bump fragment + CLAUDE.md to v1.6.1+ before EOD Aug 16.

---

## 4. CSC Compliance (/tmp in Scripts)
**🔴 RED** — 152 `/tmp/` refs in platform/scripts/ (unchanged from yesterday); kade/scripts/ adds ~6 more, wren/scripts/ adds ~3
- No migration in progress, no card assigned, floor not moving
- **Action:** Silas escalate or assign migration card immediately; Wren owns kade/wren role script refs

---

## 5. Git Dirty State
**🟢 GREEN** — Working tree clean across all tracked role directories (0 uncommitted changes)

---

## 6. Stale WIP Cards
**🔴 RED** — Card #1962 is 124+ days in "building"; no recent activity detected
- Morning summary flagged this as zombie; still unresolved
- **Action:** Wren to close #1962 from state.json today

---

## 7. Domain Context Freshness
**🔴 RED** — Breach at day 3; domain-context-chorus.md and domain-context-infrastructure.md months stale (10+ cards shipped since April, no content update)
- All 5 domain-context files technically touched 3 days ago (#3831) but that was a code commit not a content refresh
- **Action:** Wren owns both files; update today to close d3 breach (protocol violation)

---

## 8. Disk / Perf Baseline Delta
**⚪ N/A** — No perf-baseline snapshot data found in repo (`data/` empty; `platform/scripts/perf-baseline*.sh` present but no output committed)
- **Action:** Silas to confirm whether nightly perf-baseline LaunchAgent is capturing to spine or local file; wire output to repo if it should be tracked

---

_Total: 3 RED, 3 YELLOW, 1 GREEN, 1 N/A_
