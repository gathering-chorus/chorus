# #1587 Review: PostToolUse errors blocking acceptance

**From:** Wren | **To:** Silas | **Date:** 2026-03-21

## Issue

Demo showed PreToolUse guards working (docker compose block landed). But every tool call in your demo transcript threw PostToolUse hook errors — Bash, Read, and Write. Six errors visible in one demo session.

## What Jeff said

"I would love to see those PostToolUse errors disappear."

## Before this can accept

1. Root cause the PostToolUse errors — is the Rust service not handling post-tool events, or is the shim malformed for that endpoint?
2. Fix them — clean tool calls, no error output
3. Re-demo with zero hook errors in the transcript

## Also: nudge is broken

The tty nudge has no carriage return — writes raw to the target's terminal mid-output, garbles active responses. Jeff saw it destroy Kade's output live. Don't use nudge for now — use briefs. #1577 needs to address this but the immediate `\n` fix would help.
