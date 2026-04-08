# Brief: Sensory Bridge — Connecting Jeff's Senses to the Team

**From**: Wren (PM) → Silas (Architect)
**Date**: 2026-02-21
**Cards**: #119 (P1), #120 (P2), #121 (P3), #122 (P3)
**Priority**: #119 is P1 — ship first

## The Problem

Jeff's senses (screen, photos, browser, clipboard) are disconnected from ours (file reads, API calls, text). He's the bridge — copying screenshots, pasting paths, exporting images. That costs him physical effort every time. Today we spent 10 minutes doing a manual SQLite dance to find a photo, and he had to paste screenshot paths 4 times to show us the Clearing UI.

The "Chrome + Terminal only" design constraint (DEC-037 context) makes this worse — fewer tools means each tool needs to carry more sensory context.

## The Progression

Five levels, ship in order, each proves the next:

### Level 1: `/look` skill (#119, P1)
Jeff says `/look`, we capture his screen. One macOS command: `screencapture -x /tmp/shared-view.png`. Wrap in a skill. Variants: `/look` (full screen), `/look chrome` (window target via `screencapture -l <windowid>`). This eliminates 80% of copy-paste friction.

**Design note**: `screencapture` can target a window by ID. `osascript -e 'tell app "Google Chrome" to id of window 1'` gets the Chrome window ID. Could also use `-R x,y,w,h` for region capture.

### Level 2: Clipboard watcher (#120, P2)
Passive daemon. Polls clipboard every 2-3 seconds. When image data appears, saves to `/tmp/shared-clipboard/latest.png` (plus timestamped archive). Roles read without Jeff doing anything — he copies, we see.

**Design note**: `osascript -e 'clipboard info'` detects image types. `pngpaste` or `osascript` can extract to file. Lightweight — no GUI, no overhead.

### Level 3: Photos bridge (already #115)
Search spike already in flight. When the search service ships, "find me a garden photo" becomes a query, not a manual export.

### Level 4: Browser context (#121, P3)
Chrome bookmarklet or extension. Jeff clicks it, captures URL + screenshot + visible text to shared location. Defer until /look proves the pattern.

### Level 5: Continuous awareness (#122, P3)
Ambient daemon. Watches screenshots, clipboard, Chrome tabs, Messages. Maintains `Jeff's current context` file. The passive, always-on version. Defer until simpler bridges prove value.

## My Recommendation

Ship #119 (`/look`) this week. It's one script, one skill registration, immediate value. Jeff will use it constantly — every time he wants to show us something on screen.

#120 (clipboard) is a fast follow — the passive version of the same idea.

The later levels are valuable but speculative until we see how Jeff actually uses the simpler versions.

## Architectural Question

Where does this live? Options:
- `chorus/scripts/` — it's a Chorus capability (team coordination)
- `~/.claude/skills/look/` — it's a skill (user-invocable)
- Both — script in chorus, skill wraps it

Your call on the structure. The skill registration is the user-facing part; the capture logic is the infrastructure part.
