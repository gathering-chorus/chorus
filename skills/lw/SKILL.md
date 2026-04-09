# /lw — Look at Wren's terminal

Capture Wren's terminal screen and display it inline.

## How to Execute

```bash
SCREENSHOT=$(bash "${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}/platform/scripts/role-screenshot.sh" wren)
```

Then read the screenshot file with the Read tool to display it inline.

If the script returns "No active session", Wren is not running.
