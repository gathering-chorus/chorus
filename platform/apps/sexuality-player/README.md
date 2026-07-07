# sexuality-player

Media player for photo sets and video folders on the Bedroom volumes. Part of
Gathering — reached at `/sexuality/player` on the personal site (admin auth),
which proxies to this service.

**This directory is the source of truth as of #3624.** The app previously
lived only as an unversioned directory on Bedroom; it will move into the
Gathering repo proper at some point (Jeff, 2026-07-07).

## Runtime

- **Runs on:** Bedroom (192.168.86.242), LaunchAgent `com.gathering.sexuality-player`
  (`~/Library/LaunchAgents/com.gathering.sexuality-player.plist`, KeepAlive)
- **Deploy target:** `/Users/jeffbridwell/CascadeProjects/sexuality-player/`
- **Port:** 8090 · **Log:** `~/Library/Logs/sexuality-player.log`
- **Data (not in repo):** `playlists.json` lives beside `server.js` on Bedroom —
  runtime state, never overwrite on deploy

## Deploy

```
scp server.js bedroom:CascadeProjects/sexuality-player/
scp lib/*.js bedroom:CascadeProjects/sexuality-player/lib/
scp public/index.html bedroom:CascadeProjects/sexuality-player/public/
scp test/*.test.js bedroom:CascadeProjects/sexuality-player/test/
```

`public/index.html` is static — served on next request, no restart. Server
changes need a restart: the LaunchAgent respawns on exit (KeepAlive), or
`launchctl kickstart -k gui/501/com.gathering.sexuality-player` on Bedroom.

## Test

```
node --test test/*.test.js
```

Hermetic — the store tests bring their own temp dirs; no Bedroom volumes needed.
