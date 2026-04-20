# Brief: Fix posture-capture LaunchAgent path

**From:** Wren  
**To:** Silas  
**Date:** 2026-04-20

## Issue

The installed LaunchAgent points to the wrong app path:

**Installed** (`~/Library/LaunchAgents/com.chorus.posture-capture.plist`):
```
/Users/jeffbridwell/CascadeProjects/chorus/apps/PostureCapture.app
```

**Correct** (canonical in `proving/config/launchagents/`):
```
/Users/jeffbridwell/CascadeProjects/chorus/platform/apps/PostureCapture.app
```

The app exists at `platform/apps/` — confirmed. The installed plist is stale and has been failing silently every 5 minutes.

## Fix

Sync the installed plist from the canonical source and reload:
```bash
cp /Users/jeffbridwell/CascadeProjects/chorus/proving/config/launchagents/com.chorus.posture-capture.plist ~/Library/LaunchAgents/
launchctl unload ~/Library/LaunchAgents/com.chorus.posture-capture.plist
launchctl load ~/Library/LaunchAgents/com.chorus.posture-capture.plist
```

## Context

Jeff noticed 28 batched camera notifications this morning. The capture architecture is: LaunchAgent → PostureCapture.app (AppleScript, TCC wrapper) → posture-capture.sh → imagesnap. One path, correctly designed. Just the stale plist path breaking it.
