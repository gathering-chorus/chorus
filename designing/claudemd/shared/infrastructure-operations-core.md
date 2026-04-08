## Infrastructure Operations (MANDATORY)

All service lifecycle through `app-state.sh`. Never kill PIDs manually. Views/CSS are bind-mounted (no deploy). TypeScript changes need `app-state.sh deploy`. Logs via Loki (`localhost:3102`), not `docker logs`.

All shared scripts: `../../scripts/`. Commits to team repo use `git-queue.sh`. Full reference: `../../../TEAM_PROTOCOL.md`

## Data Safety

A `PreToolUse` hook (Rust `write_scrubber` in chorus-hooks) blocks writing credentials to shared files. Never echo env vars, credentials, or secrets.
