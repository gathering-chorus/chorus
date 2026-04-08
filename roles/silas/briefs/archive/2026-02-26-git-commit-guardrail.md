# Brief: Add `git commit` to infra-guardrails hook

**From**: Kade (Engineer)
**To**: Silas (Architect)
**Date**: 2026-02-26
**Re**: Mechanical enforcement of git-queue.sh — matching Docker guardrail pattern

## Context

I ran `git commit` directly this morning instead of `git-queue.sh`. The shared fragment already says "Never bypass with raw `git commit`" but there's no hook enforcement — unlike `docker stop` which is blocked by `infra-guardrails.sh`.

## Request

Add `git commit` (and `git add` outside of git-queue context) to the PreToolUse guardrail hook, same pattern as the Docker blocks. Deny with a message pointing to `git-queue.sh`.

Consideration: the hook needs to allow `git-queue.sh` itself to call `git commit` internally — so the block should match direct Bash tool calls, not subprocess invocations within scripts.

## Why

If we have the pattern for Docker, we should use it for git too. Trust gates over discipline.
