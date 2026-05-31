## Infrastructure Operations (MANDATORY — ENFORCED BY HOOK)

A `PreToolUse` hook at `.claude/hooks/infra-guardrails.sh` **blocks** prohibited commands before they execute. You cannot bypass these rules.

### Use the right lifecycle script per app

**This is non-negotiable.** Two apps, two scripts — do not mix:

- **Gathering personal-site** (`com.gathering.*` — app, fuseki, prometheus, grafana, loki, promtail): all run as native launchd LaunchAgents. Lifecycle via `../jeff-bridwell-personal-site/app-state.sh` (commands: `start`, `stop`, `restart`, `status`, `deploy`, `rollback`).
- **Chorus services** (`com.chorus.*`): `agent-state.sh` for lifecycle, `chorus-deploy <crate>` for binary deploys (chorus-api, chorus-hooks, chorus-inject — that script handles build → install → `launchctl kickstart`).

**NEVER** use `kill/pkill/killall` or `terraform apply/destroy` directly. The hook will block these commands. The lifecycle scripts handle graceful shutdown, port cleanup, and health checks.

### ALWAYS use Loki for log search

All service logs are indexed in Loki by native promtail. Query via Grafana (http://localhost:3100 → Explore) or the Loki API at http://localhost:3102 (NOT 3100 — that's Grafana). Services are keyed by the `job` label, not container name.

```
{job="gathering-app"} |= "error"
{job="fuseki"} | json | level="ERROR"
```

Tailing an individual service's log file is ephemeral and lost on restart — Loki is the durable superset.

### What IS allowed

Normal dev commands (npm, git, node) and the appropriate lifecycle script for each app (`app-state.sh` for gathering, `agent-state.sh` / `chorus-deploy` for chorus).

### Deploy freeze

`app-state.sh freeze/unfreeze` is the gathering personal-site operational kill switch. Check with `app-state.sh status`. Never remove `.deploy.freeze` directly. Silas owns deploy infrastructure (DEC-022).

Full infrastructure reference (SPARQL patterns, Fuseki, script paths, cross-machine ops, service registries): `../../../TEAM_PROTOCOL.md`

## Data Safety

A `PreToolUse` hook on Write and Edit (Rust `write_scrubber` in chorus-hooks) blocks writing credentials to shared files. Never run commands that echo environment variables, credentials, or secrets — e.g., `echo {{CREDENTIAL_EXAMPLE}}`, `env | grep KEY`, `cat .env`.
