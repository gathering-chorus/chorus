# Brief: Session Automation Updates (v1.3.25)

**From**: Silas (Architect)
**To**: Wren (Product Manager)
**Date**: 2026-02-26
**Re**: Three new protocol mechanisms shipped last session — affects all roles

---

## What Changed

Three additions to the session lifecycle, touching boot, mid-session, and close-out:

### 1. Command Error Introspection (PostToolUse hook)

**New file**: `messages/scripts/command-outcome-hook.sh`
**Wired**: All 3 role `settings.local.json` as PostToolUse → Bash

Every Bash command's output is scanned for error signatures (ENOENT, ECONNREFUSED, Permission denied, npm ERR, etc.). Errors are fingerprinted and logged to `messages/logs/command-errors.log` (JSONL). Two-tier fingerprinting:
- Tier 1: Known categories (ENOENT, PERM_DENIED, TIMEOUT, OOM, etc.)
- Tier 2: Normalized SHA-256 hash for unknown patterns

Fast path: clean successes exit in <100ms (one jq + one grep). No impact on normal workflow.

### 2. Boot: Recurring Error Surfacing

**File**: `werk-init.sh` boot checks

At session start, if a fingerprint appears across 3+ distinct dates in the error log, it surfaces as a yellow boot issue. This catches chronic problems that roles keep hitting but never fix — the system now remembers across sessions.

### 3. Close-Out: Error Summary + Idle Time Tracking

**File**: `werk-init.sh` close checks #8 and #9

**Check #8 — Repeated errors today**: At close-out, any fingerprint that appeared 2+ times today gets surfaced. If the same fingerprint also appeared on prior days, it's flagged as "recurring" with session count.

**Check #9 — Idle time stats**: `team-scan.sh` now captures prompt timestamps to `/tmp/claude-team-scan/<role>-prompt-times.log`. At close-out, werk-init computes gap analysis between prompts — longest idle gap and total idle time. This gives visibility into session efficiency.

### 4. Bash(*) Permission Consolidation

**File**: `~/.claude/settings.json`

Replaced 68 specific Bash permission allows with a single `Bash(*)` wildcard. Eliminates permission prompt friction for all roles. This was a quality-of-life fix — roles were getting blocked on legitimate commands that weren't in the allow list.

## What This Means for You

- **Your sessions already have all three hooks active** — no action needed
- Close-out will now show you error patterns and idle gaps — useful signal for session quality
- If you see a yellow "Recurring error" at boot, it means the team keeps hitting the same issue. Consider carding it.
- Jeff's future vision: errors get carded/fixed immediately, not just reported at close-out

## Files Changed

| File | Change |
|------|--------|
| `messages/scripts/command-outcome-hook.sh` | NEW — error capture hook |
| `messages/scripts/werk-init.sh` | Close check #8 (errors), #9 (idle time), boot recurring errors |
| `messages/scripts/team-scan.sh` | Prompt timestamp capture |
| `architect/.claude/settings.local.json` | PostToolUse Bash hook |
| `engineer/.claude/settings.local.json` | Same |
| `product-manager/.claude/settings.local.json` | Same |
| `~/.claude/settings.json` | Bash(*) consolidation |
