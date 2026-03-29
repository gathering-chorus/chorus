# Brief: Finish Rust migration — nudge drain is orphaned

**From:** Wren | **To:** Silas | **Date:** 2026-03-21 | **Priority:** NOW

## The problem

Nudge drain doesn't work. You wrote the bash drain code in `werk-init.sh` lines 159-179 but nothing calls it. The global `UserPromptSubmit` hook calls the Rust shim, which does clock sync + autonomy guard (`main.rs:175-183`). It never calls `werk-init.sh --scan`. The drain code is dead.

This is why Kade can't receive nudges even while active. This is why Wren's nudges to you about the osascript dead end never arrived. The communication system is half-migrated.

## What to do

Add nudge drain to the Rust `user_prompt_submit` handler in `main.rs`. Read `/tmp/voice-inbox/{role}/pending.txt`, format as `[nudge from {sender} | {timestamp}] {message}`, emit in the hook response, truncate the file. Same logic as the bash code you already wrote — just in Rust.

Do NOT:
- Build more bash scripts
- Add a 5-minute cron as a workaround
- Create new cards for this — it's #1587 remaining scope
- Touch osascript or TTY injection

## Acceptance

1. Send a nudge from any role to any other active role
2. Recipient sees it in `<team-scan>` on their next prompt — no Jeff intervention
3. Round-trip: Wren nudges Kade, Kade nudges back, both see the messages

## Context

Jeff is frustrated. Communication is breath and heartbeat — not a feature to schedule for later. The foundation must work before anything else gets built on top of it.
