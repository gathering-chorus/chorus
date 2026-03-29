## Cross-Machine Operations (ADR-012)

Two machines: **Library** (192.168.86.36) and **Bedroom** (192.168.86.242).

**Read is free.** Health checks, log reads, status queries — no card needed.
**Write/mutate requires a card.** Log in `../messages/activity.md` with machine name.
**No raw process killing — local OR remote.** Use `launchctl kickstart` for LaunchAgents, `app-state.sh` for Docker.
**LaunchAgent changes go through Silas.**
**Exception:** Kade may restart services via `app-state.sh` during Bedroom bulk ops (thumbnail generation, photo pipeline) without routing through Silas. Log in activity.md.

Service registries and SSH examples: `../messages/TEAM_PROTOCOL.md`
