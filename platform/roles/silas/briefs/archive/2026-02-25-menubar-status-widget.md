# Brief: macOS Menubar Status Widget (#391)

**From:** Wren → **To:** Silas
**Card:** #391 — Build macOS menubar status widget — three-dot role activity indicator
**Priority:** P1

## What

A SwiftUI menubar app that shows real-time role status for Wren, Silas, and Kade. Three colored dots in the macOS menubar — green (processing), amber (waiting on Jeff), gray (idle/offline). Click to expand with role names, states, active cards, and time since last activity.

## Why

Jeff monitors three terminal sessions simultaneously. There's no peripheral signal for which roles are active, blocked, or idle. He has to scan each tab. This gives him ambient awareness without context switching — a lava lamp for the team.

## Architecture

### Layer 1: Hook → Status Files

Each role's Claude Code hooks write state changes to `/tmp/role-status-{role}.json`:

```json
{
  "role": "wren",
  "state": "processing",
  "timestamp": "2026-02-25T11:26:00-05:00",
  "card": "#360",
  "card_title": "Chorus landing page"
}
```

**Hook wiring** (add to each role's `.claude/settings.local.json`):

| Hook Event | State Written |
|-----------|---------------|
| SessionStart | `processing` |
| UserPromptSubmit | `processing` |
| Stop | `idle` (or `waiting` if last message was a question / PermissionRequest pending) |
| PermissionRequest | `waiting` |
| SessionEnd | `offline` |

The hook script is shared: one script at `messages/scripts/role-status-hook.sh` that reads the event type and role from args/env and writes the JSON.

### Layer 2: SwiftUI Menubar App

- Watches `/tmp/role-status-*.json` via FSEvents (no polling)
- Renders three dots in menubar: 🟢🟡⚫
- Click expands to show role detail
- LaunchAgent managed (`com.chorus.role-status`)
- ~100-150 lines of Swift

### Color mapping

| State | Dot | Meaning |
|-------|-----|---------|
| `processing` | 🟢 green | Role is actively working |
| `waiting` | 🟡 amber | Blocked on Jeff (permission prompt or question) |
| `idle` | ⚪ gray | Session open but not active |
| `offline` | ⚫ black | No session running |

## Acceptance Criteria

1. Three dots visible in macOS menubar with correct state colors
2. Click shows dropdown: role emoji + name + state + active card + time since last update
3. State updates within 2 seconds of hook firing
4. Works on Library Mac (primary dev machine)

## Constraints

- No new services or ports — file-based only
- Hook script must be idempotent (multiple fires = same result)
- App should be lightweight — no Electron, no web stack
- LaunchAgent plist follows existing `com.chorus.*` naming

## Sizing Estimate

- Hook script: small (shared, ~30 lines bash)
- Hook wiring: small (settings.json edits per role)
- SwiftUI app: medium (~150 lines)
- LaunchAgent: small

Total: **medium** — probably 1-2 sessions.

## Questions for Silas

1. Any concerns with FSEvents watching `/tmp/`? Alternative location?
2. Should the app live in the Chorus repo or its own?
3. Do you want to handle the `waiting` vs `idle` distinction in the hook script or the app?
