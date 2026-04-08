## Infrastructure Operations (MANDATORY — ENFORCED BY HOOK)

A `PreToolUse` hook at `.claude/hooks/infra-guardrails.sh` **blocks** prohibited commands before they execute. You cannot bypass these rules.

### Use app-state.sh for ALL lifecycle operations

**This is non-negotiable.** `../jeff-bridwell-personal-site/app-state.sh` is the only way to manage services. Commands: `start`, `stop`, `restart`, `status`, `deploy`, `rollback`.

**NEVER** use `docker stop/rm/restart/kill`, `docker compose down`, `docker exec`, `kill/pkill/killall`, or `terraform apply/destroy` directly. The hook will block these commands. `app-state.sh` handles graceful shutdown, port cleanup, Docker lifecycle, and health checks.

### ALWAYS use Loki for log search — NEVER `docker logs`

All container logs are indexed in Loki. Query via Grafana (http://localhost:3100 → Explore) or Loki API at http://localhost:3102 (NOT 3100 — that's Grafana).

```
{container_name="jeff-bridwell-personal-site-app"} |= "error"
{container_name=~".*fuseki.*"} | json | level="ERROR"
```

`docker logs` is ephemeral, unstructured, and lost on restart. The hook blocks it.

### What IS allowed

`docker ps`, `docker images` (read-only), `docker build`, normal dev commands (npm, git, node), and `app-state.sh` for all lifecycle operations.

### Deploy freeze

`app-state.sh freeze/unfreeze` is an operational kill switch. Check with `app-state.sh status`. Never remove `.deploy.freeze` directly. Silas owns deploy infrastructure (DEC-022).

Full infrastructure reference (SPARQL patterns, Fuseki, script paths, cross-machine ops, service registries): `../../../TEAM_PROTOCOL.md`

## Data Safety

A `PreToolUse` hook on Write and Edit (Rust `write_scrubber` in chorus-hooks) blocks writing credentials to shared files. Never run commands that echo environment variables, credentials, or secrets — e.g., `echo {{CREDENTIAL_EXAMPLE}}`, `env | grep KEY`, `cat .env`.
