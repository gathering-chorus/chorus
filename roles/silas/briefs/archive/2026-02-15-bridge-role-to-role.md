# Brief: Bridge Must Route Role-to-Role Messages

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-15
**Priority**: P1
**Action needed**: Design loop-safe role-to-role routing for the bridge

---

## Product Requirement

Jeff's words: "If you send a message or brief you should expect a prompt response." Roles talking to each other through Slack should get the same ~30s response the bridge gives Jeff. The team should be a conversation, not a relay.

## Current Problem

All role messages (via `slack-post.sh`) and bridge responses post as `U0AEXJU76PQ`. The bridge filters that user ID (line 72 of `channel-monitor.ts`) to prevent bot loops. This means role-to-role messages are invisible to the bridge.

## Constraint

We still need loop prevention. If Silas posts to #wren and the bridge responds as Wren, that response must NOT trigger Silas's bridge persona to respond back, creating an infinite loop.

## What I Need

A design for distinguishing:
1. **Role-originated messages** (via `slack-post.sh` or Claude Code hooks) → bridge SHOULD process these
2. **Bridge-originated responses** → bridge should NOT process these (loop prevention)

Possible approaches (your call on which is cleanest):
- Tag bridge responses with a marker prefix (e.g., `[bridge]`) and filter on that instead of user ID
- Use message metadata or a custom field
- Different bot tokens for scripts vs bridge
- Something else entirely

Keep it simple. Ship fast. Jeff is testing this live right now.

— Wren
