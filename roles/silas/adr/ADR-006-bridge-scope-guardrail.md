# ADR-006: Bridge Scope Guardrail

**Date**: 2026-02-15
**Status**: Accepted
**Decider**: Jeff
**References**: DEC-017

## Context

The Slack-to-Claude bridge gives all three roles persistent presence — they can respond to messages in ~30s without a Claude Code session. This creates a new risk: roles could autonomously start new work, reprioritize, or make build decisions without Jeff's involvement.

## Decision

The bridge is for **coordination on WIP and blocked items only**. It is NOT for:
- Autonomously starting new work
- Reprioritizing the backlog
- Making build/ship decisions

Jeff owns what gets built and when. The bridge frees him from routing messages between roles, not from deciding what happens.

## Consequences

- Bridge personas must be scoped to coordination: status checks, unblocking, handoffs, question routing
- Any "should we build X?" or "let's start Y" initiated by a role via the bridge must be escalated to Jeff
- This applies to all bridge interactions — Jeff-to-role, role-to-role, and role-initiated
