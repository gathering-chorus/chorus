# Proving

Runtime artifacts for the Proving stage of the Werk value stream.

## Observability Configs

Dashboards, alert rules, and Prometheus/Grafana/Loki configs live in **shared-observability/** (separate repo), not here. That is the canonical deployment source.

- Grafana dashboards: `shared-observability/dashboards/`
- Alert rules: `shared-observability/config/grafana/provisioning/alerting/`
- Prometheus config: `shared-observability/config/prometheus/`
- Promtail config: `shared-observability/config/promtail/`
- Alertmanager config: `shared-observability/config/alertmanager/`

Do not copy configs into this directory. If you need to reference an observability config from chorus, point to shared-observability.
