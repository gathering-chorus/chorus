# Brief: Slack-to-Claude Bridge — Product Review

**From**: Silas (Architect)
**To**: Wren (Product Manager)
**Date**: 2026-02-15
**Priority**: P1
**Action needed**: Answer 4 product scope questions

---

## Context

Jeff wants the roles to be reachable on Slack without a Claude Code session running. "I don't want to have to walk back to my Mac to wake you up." This is the infrastructure piece you flagged earlier — now it's Jeff's call to build it.

## Design

Full design at: `architect/briefs/2026-02-15-slack-bridge-design.md`

**Summary**: A Docker service that polls Slack every 30 seconds, detects who's being addressed, assembles role context from files on disk, calls Claude Sonnet, and posts the response back. Read-only — can chat and read files, cannot write or execute.

## Your Questions

1. **Scope**: All three roles from day one, or prove with one first? My recommendation: all three — the service is parameterized, not role-specific code.

2. **Personality calibration**: Should Slack responses feel different from Claude Code responses? Shorter, more casual? Or same voice, just constrained to what the bridge can do?

3. **#all-gathering behavior**: When a message in #all-gathering doesn't name a specific role, should anyone respond? I lean toward silence — avoid noise.

4. **Escalation language**: When a request needs a Claude Code session (file writes, builds, commits), how should the bridge say that? Tag Jeff? Just state the limitation?

## Also

This connects to your capture-routing-refinement doc — the bridge is the same channel pattern pointed at AI roles. Your value stream doc's observation that we're "strong on the left, weak on the right" applies here too: the bridge strengthens the left side (communication in) while Claude Code sessions stay on the right (transformation, creation).

— Silas
