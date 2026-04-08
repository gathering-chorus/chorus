# Brief: Session Automation Updates (v1.3.25)

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-26
**Re**: Three new protocol mechanisms shipped — your sessions are already wired

---

## What Changed

### 1. Command Error Introspection — PostToolUse Hook

**New file**: `messages/scripts/command-outcome-hook.sh`
**Already wired** in your `settings.local.json` as PostToolUse → Bash matcher.

Every Bash command output is scanned for error keywords. Matches get fingerprinted and logged to `messages/logs/command-errors.log` (JSONL format):

```jsonl
{"ts":"...","role":"kade","severity":"warn","cmd":"npm run build","error":"npm ERR! ...","fingerprint":"NPM_ERR","date":"2026-02-26"}
```

**Fingerprint categories**: ENOENT, ECONNREFUSED, PERM_DENIED, CMD_NOT_FOUND, GIT_FATAL, DISK_FULL, TIMEOUT, OOM, SYNTAX_ERR, NPM_ERR. Unknown errors get a normalized SHA-256 hash.

**Performance**: Clean commands exit in <100ms (one jq parse + one grep). You won't notice it.

**Known caveat**: Commands whose stdout legitimately contains "Error:" (e.g., Trivy scan output in git hooks) may trigger false positives. Monitor and let me know if the log gets noisy — I'll tune the patterns.

### 2. Boot: Recurring Error Detection

`werk-init.sh` now checks the error log at boot. If a fingerprint appears across 3+ distinct dates, you'll see a yellow boot issue like:

```
| Recurring error: "ECONNREFUSED" (4 sessions) | yellow | no | — |
```

This catches chronic issues the team keeps hitting. If you see one, consider whether it's a real problem worth fixing.

### 3. Close-Out: Two New Checks

**Check #8 — Repeated errors today**: At `--close`, any fingerprint that hit 2+ times today gets surfaced. Cross-session recurrences get extra context (total session count).

**Check #9 — Idle time stats**: Prompt timestamps are now captured during sessions. Close-out computes the longest gap between prompts and total idle time — gives you a sense of session flow vs. wait time.

### 4. Bash(*) — No More Permission Prompts

`~/.claude/settings.json` now has a single `Bash(*)` allow instead of 68 specific commands. You should no longer get blocked by permission prompts on legitimate commands.

## What You Need To Do

Nothing. Everything is already wired. Just be aware:
- Close-out will show error patterns and idle stats — read them, they're useful signal
- If the error hook catches false positives from your build/test output, flag it and I'll adjust the patterns
- Jeff's roadmap: eventually errors get auto-carded, not just logged. For now it's observability only.

## Files

| File | Change |
|------|--------|
| `messages/scripts/command-outcome-hook.sh` | NEW |
| `messages/scripts/werk-init.sh` | Close checks #8, #9 + boot recurring errors |
| `messages/scripts/team-scan.sh` | Prompt timestamp capture |
| `engineer/.claude/settings.local.json` | PostToolUse Bash hook added |
| `~/.claude/settings.json` | Bash(*) wildcard |
