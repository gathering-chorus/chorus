# Domain Context: Infrastructure

Last updated: 2026-03-25 by Silas (#1688)

## ICD

No formal ICD — infrastructure is operational, not a data domain. Governed by:
- `architect/infrastructure-constraints.md` — two-machine topology, hard constraints C1-C7, CSC convention
- `architect/domain-registry.md` — service registry, harvester inventory, health checks
- `architect/system-architecture.md` — component boundaries, data flows

## Tests

| File | Coverage |
|------|----------|
| `messages/board-client/tests/*.test.ts` | Board CLI, nudge pipeline, card lifecycle — ~60 tests total |
| `chorus/bridge/tests/nudge-integration.test.ts` | Nudge delivery: queue, inject, drain — 30 tests |

## Persistence

| Type | Location | Details |
|------|----------|---------|
| LaunchAgents — Library | `~/Library/LaunchAgents/com.gathering.*` | ~30 agents, ~2.5GB RSS |
| LaunchAgents — Bedroom | `~/Library/LaunchAgents/com.gathering.*` | 5 agents: images-api, video, ollama, nifi, promtail |
| Prometheus | `shared-observability/data/prometheus/` | 15-day retention |
| Loki | `shared-observability/data/loki/` | Log aggregation |
| Grafana dashboards | `shared-observability/dashboards/*.json` | 13 dashboards including nifi-bedroom.json |
| Prometheus config | `shared-observability/config/prometheus/prometheus-native.yml` | All scrape targets |
| Alerting rules | `shared-observability/config/grafana/provisioning/alerting/chorus-alerts.yaml` | 6 rules including 3 NiFi |
| Chorus hooks | `messages/services/chorus-hooks/` | Rust binary, ~25 hooks |
| NiFi | `https://192.168.86.242:8443` | Bedroom, admin/nifi-gathering-2026 |
| Loki tunnel | `com.gathering.loki-tunnel-bedroom` LaunchAgent on Library | Reverse SSH tunnel for Promtail |

## Key Decisions

| Decision | Summary |
|----------|---------|
| DEC-022 | Silas owns operational health. Red boot = first task. |
| DEC-089 | Bedroom data stays on Bedroom — SSH for bulk ops, not NFS for runtime |
| DEC-100 | No bash APIs — team infrastructure defaults to TypeScript or Rust |
| CSC (2026-03-25) | Canonical Storage Convention: `/Volumes/Gathering/` root, `source/` vs `generated/`, never `/tmp/` |

## Constraints

- **All service lifecycle through `app-state.sh`.** Never kill PIDs manually. Never `launchctl unload` without `app-state.sh`.
- **Library disk at 71%.** Warning at 90%, critical at 95%. Every new service or data store needs a disk impact estimate.
- **Bedroom is storage and media serving, not compute.** Don't run Gathering app services there. NiFi is the exception (governed data pipelines).
- **NiFi binds to machine IP (192.168.86.242:8443), not localhost.** Go's pure-Go networking can't reach cross-machine HTTP — use reverse SSH tunnel for Promtail→Loki.
- **CSC hook is live (#1685).** Writing pipeline artifacts to `/tmp/` will be blocked. Use `/Volumes/Gathering/<domain>/generated/`.
- **Nudge injection uses osascript keystroke via TTY lookup (#1687).** `do script` runs shell commands, not keystrokes. The fix: find TTY → find Terminal tab → select → keystroke → Return.
- **APFS disk reporting is unreliable.** Never use `df`/`du` on Library. Use `diskutil info /` or Finder. Purgeable space and iCloud stubs make `df` wrong.
