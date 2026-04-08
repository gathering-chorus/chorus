# Brief: Boot-Order Orchestration Shipped (#382)

**From**: Silas (Architect) → Kade (Engineer)
**Date**: 2026-02-25
**Priority**: FYI — operational change

## What Changed

After this morning's reboot cascade, we built automated boot-order orchestration for both Macs.

### Library (Primary)
- **Docker Desktop auto-start** enabled — launches on login
- **`docker-startup.sh`** — staged boot script: observability → infra → app, with health gates between phases and host port validation
- **LaunchAgent** (`com.chorus.docker-services`) fires on login, runs the boot script
- **`app-state.sh boot`** — new command, runs the same script manually

### Bedroom (Secondary)
- **Two LaunchAgents deployed and loaded:**
  - `com.gathering.images-api-server` — `server.js` on port 3001, `KeepAlive: true`
  - `com.gathering.images-api-video` — `video-server.js` on port 8082, `KeepAlive: true`
- Both auto-start on login and auto-restart on crash
- **Your video-server on 8082 is covered** — no more manual `node video-server.js` after reboot

### What You Need to Know
- `app-state.sh boot` brings up everything across all 4 compose stacks in dependency order
- The boot script validates Bedroom connectivity (3001 + 8082) as Stage 5
- Bedroom services are now operable from Library via SSH + launchctl
- If you need to restart the video-server: `ssh jeffbridwell@192.168.86.242 "launchctl kickstart -k gui/$(id -u)/com.gathering.images-api-video"`

### No Action Required
This is informational. Your workflow doesn't change — the video-server just won't disappear on reboot anymore.
