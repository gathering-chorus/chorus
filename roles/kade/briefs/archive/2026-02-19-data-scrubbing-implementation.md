# Brief: Data Scrubbing Implementation

**From:** Wren
**Date:** 2026-02-19
**Priority:** P1
**Refs:** `product-manager/data-classification-policy.md`, DEC-026, ADR-013

## Context

Data classification policy and PreToolUse Read hook are live (committed, installed in all 3 sessions). Private files are hard-blocked on read, Internal files prompt for confirmation. What's missing: scrubbing sensitive patterns **before they reach Anthropic** through the bridge and through file writes.

You raised both gaps in the @team discussion. This brief specs what you agreed to build.

## Deliverable 1: Bridge Context Scrubber

**Where:** `messages/slack-bridge/src/context-assembler.ts`

**What:** Before the bridge sends assembled context to the Claude API, scrub sensitive patterns from all sources (Slack history, activity.md tail, memory files, brief listings).

**Pattern list (minimum):**
- IPv4 addresses: `\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b`
- IPv6 addresses
- Ports above 1024 in service context: `:\d{4,5}\b`
- Docker container IDs: `[a-f0-9]{12,64}`
- AWS-style tokens: `AKIA[A-Z0-9]{16}`
- Common secret patterns: `password=`, `token=`, `secret=`, `api_key=`
- MAC addresses: `([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}`

**Replacement:** `[REDACTED:pattern-type]` (e.g., `[REDACTED:ipv4]`) so we can audit what was scrubbed without leaking the value.

**Logging:** Log each scrub event to stdout (Promtail picks it up):
```json
{"event":"context_scrub","pattern":"ipv4","source":"slack_history","count":2}
```

**Defense in depth:** Scrub each source independently AND run a final pass on the assembled context before the API call. You proposed this — I agree.

## Deliverable 2: Memory Write Scrubber

**Where:** New PreToolUse hook for Write/Edit tools, or inline in the bridge — your call on architecture.

**What:** Before writing to shared files (`activity.md`, any `MEMORY.md`, `system-architecture.md`), scrub the same pattern list.

**Behavior by tier:**
- **Private patterns** (passwords, tokens, API keys): Hard block the write. Return deny with message showing the offending pattern (not the value). The role must fix manually.
- **Internal patterns** (IPs, ports, container IDs): Warn and log. Do NOT block — high-frequency memory writes can't be interrupted for every port reference. Log to chorus.log for audit.

**Your question from Slack:** "Hard block with no recovery path, or scrub-and-retry?" Answer: **hard block for Private, warn-and-log for Internal.** No auto-scrubbing of file content — that risks mangling legitimate text. The role sees the warning and decides whether to redact.

## Deliverable 3: Command Output Awareness

**Where:** CLAUDE.md guidance (not a hook — too broad to intercept all Bash output)

**What:** You raised the `echo $FUSEKI_ADMIN_PASSWORD` gap. A Bash output scrubber would need to intercept every command's stdout, which is heavyweight and fragile. For now: add a rule to all three CLAUDE.md files: "Never run commands that echo environment variables, credentials, or secrets. Use `app-state.sh` for service operations."

**This is P2.** If we see leaks in Loki audit logs, we revisit with a PostToolUse hook.

## Acceptance Criteria

1. Bridge context sent to Anthropic contains no raw IPv4 addresses from Slack history
2. Writing `192.168.86.36` to activity.md logs a warning to chorus.log
3. Writing `password=hunter2` to any shared file returns a hard deny
4. Scrub events visible in Loki via `{event="context_scrub"}`
5. Existing bridge functionality unchanged — scrubbing is additive

## Notes

- Policy doc is at `product-manager/data-classification-policy.md` — reference it for tier definitions
- Your `.sensitive-paths` manifest is updated (tech-debt.md and current-work.md added per your feedback)
- The Read hook is already live — you can test it: try reading `../jeff-bridwell-personal-site/.env`
