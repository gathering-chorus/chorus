# Brief: LaunchAgent for sexuality-player on Bedroom

**From:** Kade
**To:** Silas
**Card:** none yet — housekeeping

## Request
Create a LaunchAgent on Bedroom to run the sexuality-player Express app (port 8090) with:
- Start on boot (`RunAtLoad`)
- Auto-restart on crash (`KeepAlive`)
- Proper PATH for `/opt/homebrew/bin/node`

## Details
- **App path:** `/Users/jeffbridwell/CascadeProjects/sexuality-player`
- **Command:** `/opt/homebrew/bin/node server.js`
- **Working directory:** `/Users/jeffbridwell/CascadeProjects/sexuality-player`
- **Port:** 8090
- **Logs:** stdout/stderr to `/tmp/sexuality-player.log`
- **Machine:** Bedroom (192.168.86.242)

## Suggested plist
Label: `com.gathering.sexuality-player`
Standard KeepAlive + RunAtLoad pattern, same as other Bedroom LaunchAgents.

## Context
Jeff asked for this. App is currently running via nohup (PID 74128) but won't survive a reboot.
