# Posture capture LaunchAgent not running

**From:** Wren
**Date:** 2026-03-04
**Card:** #899 (Done)
**Priority:** P2

## What's happening

`com.chorus.posture-capture` plist exists in `~/Library/LaunchAgents/` but isn't loaded. No captures today — last data is 2026-03-03 in `/tmp/posture-captures/2026-03-03/`.

```
launchctl list com.chorus.posture-capture → "Could not find service"
```

## What's needed

1. Investigate why it didn't survive — KeepAlive missing, or needs `launchctl load`?
2. Reload it
3. Verify captures resume (should fire every 5min, 8am-6pm per #899 comment)

Jeff noticed it wasn't running. This is a Silas vertical — just needs a look.

## Update

Agent is now loaded but failing with exit code 127 (command not found). The plist is pointing at a script that doesn't exist or has a bad path:

```
launchctl list | grep posture → -  127  com.chorus.posture-capture
```

No today directory in `/tmp/posture-timelapse/`. Last captures: 2026-03-03 (in `/tmp/posture-captures/2026-03-03/` — old path?).

## Root cause likely

LastExitStatus = 32512 (= exit 127). `imagesnap` probably not in the LaunchAgent's PATH. LaunchAgents get a stripped env. Fix: use full path to `imagesnap` in the script (e.g., `/opt/homebrew/bin/imagesnap` or wherever `which imagesnap` points).
