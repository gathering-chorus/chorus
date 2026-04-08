# /lk — Look at Kade's terminal

Capture Kade's terminal screen and display it inline.

## How to Execute

```bash
SCREENSHOT=$(bash "${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/role-screenshot.sh" kade)
```

Then read the screenshot file with the Read tool to display it inline.

If the script returns "No active session", Kade is not running.
