## Infrastructure Operations (MANDATORY)

Two apps, two lifecycle paths — do not mix them:

- **Chorus services** (`com.chorus.*` — chorus-api, chorus-hooks, chorus-clearing, etc.): `agent-state.sh` for lifecycle (start/stop/restart/status/deploy/rollback). Binary-backed services (chorus-hooks, chorus-inject) and chorus-api deploy through `chorus-deploy <crate>`, which handles build → install → `launchctl kickstart com.chorus.<service>`. chorus-api restarts happen in `chorus-deploy`, not in either *-state script.
- **Gathering personal-site** (`com.gathering.*` — app, fuseki, prometheus, grafana, loki, promtail): `app-state.sh` for lifecycle. Hardcoded to the gathering stack; do not call it for `com.chorus.*` services.

Never kill PIDs manually. Views/CSS are bind-mounted (no deploy). TypeScript changes need a `deploy` verb on the appropriate script. Logs via Loki (`localhost:3102`), not `docker logs`.

All shared scripts: `../../scripts/`. Commits to team repo use `git-queue.sh`. Full reference: `../../../TEAM_PROTOCOL.md`

## Data Safety

A `PreToolUse` hook (Rust `write_scrubber` in chorus-hooks) blocks writing credentials to shared files. Never echo env vars, credentials, or secrets.
