---
name: ot
description: Open in tab — open file or URL in the role's own Chrome window
user-invocable: true
---

# ot — Open in Tab

Open a file or URL in the role's own Chrome window. Never opens in Jeff's window.

## How to Use

1. Determine which role you are (wren, silas, or kade)
2. Run:

```bash
/Users/jeffbridwell/CascadeProjects/platform/scripts/chrome-window.sh <role> <target>
```

Target can be:
- A URL: `ot http://localhost:3000/about/SYSTEM_MODEL`
- A local file: `ot file:///path/to/file.html`
- A relative path: resolve from current working directory first, prefix with `file://`

The script finds or creates the role's Chrome window (tracked by saved window ID) and navigates to the URL.

Use `/lc` to see Jeff's Chrome. Use `/ot` to show things in your own window.
