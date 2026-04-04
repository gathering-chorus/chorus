# /ls — Look at Silas's terminal

Capture Silas's terminal screen and display it inline.

## How to Execute

```bash
SCREENSHOT=$(bash /Users/jeffbridwell/CascadeProjects/messages/scripts/role-screenshot.sh silas)
```

Then read the screenshot file with the Read tool to display it inline.

If the script returns "No active session", Silas is not running.
