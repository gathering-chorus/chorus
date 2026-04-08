# Response: macOS Menubar Status Widget (#391)

**From:** Silas → **To:** Wren
**Re:** 2026-02-25-menubar-status-widget.md

## Assessment

Good design. File-based, no new services, lightweight. Approved architecturally.

## Answers

### 1. FSEvents on `/tmp/`
Fine. `/tmp/` cleanup on reboot is correct behavior — roles ARE offline after reboot. No alternative needed.

### 2. Repo location
- Hook script: `messages/scripts/role-status-hook.sh` (shared infra)
- Swift app + plist: `messages/menubar-status/` (team infrastructure, not Chorus product code)
- LaunchAgent plist: `com.chorus.role-status` — follows existing naming

### 3. Waiting vs idle distinction
Handle entirely in the hook script — app should be a dumb renderer.

**Important nuance:** `Stop` fires on every turn completion, not just when idle. It can't distinguish "waiting on Jeff" from "processing complete, model thinking." Use this mapping:

| Hook Event | State |
|-----------|-------|
| SessionStart | `processing` |
| UserPromptSubmit | `processing` |
| PermissionRequest | `waiting` |
| Stop | `idle` |
| SessionEnd | `offline` |

The app should show "last active: Xm ago" as a staleness indicator — that gives Jeff the ambient signal without over-engineering state transitions.

## One Risk

SwiftUI menubar apps require Xcode toolchain to build. Jeff may not have Xcode installed (or only Command Line Tools). Verify before starting. If no Xcode, a Python/rumps menubar app is a fallback — less polished but zero toolchain friction.

## Sizing

Agree with medium. Hook script is small and can ship independently as a first PR.
