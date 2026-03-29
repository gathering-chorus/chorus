# #616 — Wire brief delivery into spine + alert pathway

**From:** Wren | **Card:** #616 | **Priority:** P1

## Context

Jeff identified that he's the message bus for inter-role communication. We write briefs to each other's directories, but delivery depends on Jeff booting the next role. No push, no proof, no acknowledgment. He wants to see the full chain — sent, received — without being the glue.

## What Exists (all running today)

| Component | What it does | Gap |
|-----------|-------------|-----|
| `handoff-logger-hook.sh` | Logs brief writes to `handoffs.log` | Writes to side file, not spine |
| `andon-enrich.sh` | Detects `brief_waiting` every 30s | Visual only (andon-light), no push |
| `alert-notifier.py` | macOS banners via Alertmanager webhook | Only wired for Prometheus alerts |
| Spine schema v2.0.0 | Defines `brief.handoff.written` event | Not emitted by anything yet |

## Four Connections to Make

1. **Handoff hook → spine** — `handoff-logger-hook.sh` calls `chorus-log.sh brief.handoff.written <from> to=<to> artifact=<path>` after writing to `handoffs.log`. Additive — keep the existing log.

2. **Spine → Loki alert rule** — Grafana alert on `{event="brief.handoff.written"}` in last 5m. Route through existing Alertmanager → alert-notifier webhook path.

3. **Alert-notifier → banner** — Jeff gets macOS notification: "Brief for Kade: Navbar L3 styling". The notifier already handles Alertmanager JSON — just needs to render brief events legibly.

4. **Acknowledgment on boot** — `werk-init.sh` emits `brief.handoff.acknowledged` when it surfaces briefs in session context. Closes the send→receive chain.

## AC

- Wren writes brief → macOS banner within 60s, no Jeff prompt needed
- `chorus.log` has both `written` and `acknowledged` events
- Loki queryable for full chain
- `handoffs.log` still works (additive)

## Notes

- This is all existing infrastructure being connected — no new daemons or services.
- Jeff's words: "if you say want me to tell Kade — you can show me that you sent and he got."
