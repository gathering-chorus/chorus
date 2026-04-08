---
name: look
description: Capture Jeff's screen so you can see what he sees. Eliminates copy-paste friction for visual context sharing.
user-invocable: true
---

# /look — Sensory Bridge

This skill captures Jeff's screen (or a specific window) so you can see what he's looking at. It eliminates the friction of Jeff manually screenshotting, saving, and pasting paths.

## How to Use

When Jeff invokes `/look`, capture his screen and read the image:

### Step 1: Capture

```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/look.sh [target]
```

**Targets:**
- `screen` (default) — full screen capture
- `chrome` — full screen (Chrome-specific capture not needed)
- `terminal` — full screen
- `/path/to/image.png` — use an existing image file (no capture needed)

The script outputs the path to the captured image.

### Step 2: Read the image

Use the Read tool on the captured image path. Claude's vision capability will describe what's in the image.

```
Read tool → /tmp/chorus-look/latest.png
```

### Step 3: Respond

Describe what you see. Be specific — name UI elements, text content, colors, layout. Jeff is showing you this because he wants you to understand his visual context.

**If Jeff says `/look` with no target**, capture the full screen.

**If Jeff says `/look chrome`**, capture the screen (Jeff's Chrome will be visible).

**If Jeff pastes a path** like `/look /tmp/screenshot.png`, skip capture and read the file directly.

## Privacy Note

Screenshots may contain sensitive information (passwords, emails, financial data). Never persist the raw screenshot beyond `/tmp/chorus-look/`. Only reference descriptions in conversation — do not write screenshot content to state files, briefs, or Chorus index.

## Examples

Jeff: `/look`
→ Capture full screen, read image, describe what you see

Jeff: `/look chrome`
→ Capture screen, read image, describe what you see

Jeff: "Can you see my screen?" or "Look at this"
→ Same as `/look` — capture and describe

Jeff: `/look /tmp/some-file.png`
→ Read the existing file, describe it

## When to Use Proactively

If Jeff seems to be describing something visual ("the dashboard shows...", "I'm looking at..."), suggest running `/look` so you can see it too. Don't wait for him to remember the command.
