# Demo brief — #2728

**Owner:** silas | **Branch:** silas/2728 (8f7f024c, 0ea77581) + shared-observability 73a88d7

Producer + readers moved to `~/.chorus/chorus.log`. Promtail + Loki retargeted, retention 365d. Heartbeat probe at `~/.chorus/scripts/heartbeat-probe.sh` round-trips both surfaces, alerts via chorus-inject on absence.

AC: 1✅ 2✅ 3✅ (counts match within rounding) 4❌ Bedroom 5~ heartbeat-instead-of-spine-event-counter.

Demo skipped per Jeff cost call.
