# Spike: Remote Dev Workflow for images-api (#392)

**From:** Silas | **Date:** 2026-02-25

## Findings

### Current State
- Code: `CascadeProjects/personal-website/` on both machines (GitHub: WJeffBridwell/personal-website)
- Services: `server.js` (port 3001, gallery UI) + `video-server.js` (port 8082, media serving)
- Runtime: Bare Node.js, no Docker, no build step
- Process mgmt: LaunchAgents (`com.gathering.images-api-server`, `com.gathering.images-api-video`)
- Logs: `/tmp/images-api-server.log`, `/tmp/images-api-video.log` (via LaunchAgent stdout/stderr)
- Library already has a checkout at the same path
- Both machines share the same GitHub remote

### Options Evaluated

| Option | Approach | Pros | Cons |
|--------|----------|------|------|
| **A. rsync + restart** | Edit on Library, rsync to Bedroom, launchctl restart | Simple, fast, no git coordination | Bypasses git, drift risk |
| **B. git push/pull** | Edit on Library, push, SSH pull on Bedroom, restart | Clean, auditable | More steps, merge conflicts possible |
| **C. SSH remote edit** | Claude Code / editor SSH into Bedroom directly | Zero deploy friction | Claude Code doesn't natively support remote SSH context |
| **D. File watcher** | fswatch on Library → auto-rsync → auto-restart | Hands-free | Complex, fragile, overkill for 2 files |

### Recommendation: Option B (git-mediated) with a deploy script

**Why:** Auditable, works with existing infra, prevents drift between machines. The deploy script wraps the multi-step flow into one command.

### Proposed Script: `images-api-deploy.sh`

Location: `messages/scripts/images-api-deploy.sh` (shared infra)

```bash
#!/usr/bin/env bash
# Deploy images-api from Library to Bedroom
# Usage: images-api-deploy.sh [server|video|both]
set -euo pipefail

BEDROOM="jeffbridwell@192.168.86.242"
REMOTE_DIR="/Users/jeffbridwell/CascadeProjects/personal-website"
TARGET="${1:-both}"

# 1. Push from Library
echo "Pushing to GitHub..."
cd /Users/jeffbridwell/CascadeProjects/personal-website
git push origin main

# 2. Pull on Bedroom
echo "Pulling on Bedroom..."
ssh "$BEDROOM" "cd $REMOTE_DIR && git pull origin main"

# 3. Restart service(s) via launchctl
UID_BEDROOM=$(ssh "$BEDROOM" "id -u")
case "$TARGET" in
  server) ssh "$BEDROOM" "launchctl kickstart -k gui/$UID_BEDROOM/com.gathering.images-api-server" ;;
  video)  ssh "$BEDROOM" "launchctl kickstart -k gui/$UID_BEDROOM/com.gathering.images-api-video" ;;
  both)
    ssh "$BEDROOM" "launchctl kickstart -k gui/$UID_BEDROOM/com.gathering.images-api-server"
    ssh "$BEDROOM" "launchctl kickstart -k gui/$UID_BEDROOM/com.gathering.images-api-video"
    ;;
esac

# 4. Health check
sleep 2
echo "Health check..."
ssh "$BEDROOM" "curl -s -o /dev/null -w 'server: HTTP %{http_code}\n' http://localhost:3001/health 2>/dev/null || echo 'server: unreachable'"
ssh "$BEDROOM" "curl -s -o /dev/null -w 'video:  HTTP %{http_code}\n' http://localhost:8082/health 2>/dev/null || echo 'video: unreachable'"
```

### Log Tailing from Library

```bash
# Tail both services
ssh jeffbridwell@192.168.86.242 "tail -f /tmp/images-api-server.log /tmp/images-api-video.log"
```

Could also add as an `app-state.sh`-style wrapper: `images-api-deploy.sh logs [-f]`

### Missing Pieces (future cards)
- **Health endpoints**: Verify both services actually have `/health` routes (video-server may not)
- **npm install coordination**: If package.json changes, need `npm install` on Bedroom before restart
- **Monitoring**: No Prometheus metrics from Bedroom services yet (no node-exporter there)

## Sizing

Deploy script: **small** (one session). Monitoring gap: separate card.
