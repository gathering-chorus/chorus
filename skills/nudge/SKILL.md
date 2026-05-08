---
name: nudge
description: Send a message to another role's active terminal session — immediate delivery, no brief delay.
user-invocable: true
---

# /nudge — Send Message to Active Role

Inject a message directly into another role's terminal session. The message appears after their current turn completes. Use for time-sensitive coordination that can't wait for next session.

## Arguments

```
/nudge <role> <message>
```

- `role` — silas, kade, or wren
- `message` — what you want to tell them (will be prefixed with your role name)

## How to Execute

Call the MCP tool. One call. The substrate handles the rest.

```
mcp__chorus-api__chorus_nudge_message({ to: "<role>", message: "<your-role-name>: <message>" })
```

The MCP server:
- Persists the nudge to pulse (`/api/nudge`)
- Pulse's DeliveryWorker injects via `chorus-inject` to the recipient's active Terminal window
- If the role is busy, the injection is delivered after their current turn
- If no active window, the nudge is still persisted — recipient sees it on next session via pulse drain

Returns `nudge sent: <from> → <to>` on success, or a typed refusal (e.g. `recipient-not-found`).

## What changed (#2804/#2808/#2809)

Pre-#2804 this skill called `bash platform/scripts/nudge`. That path is gone — bash script deleted, `chorus-hook-shim nudge` retired, direct `chorus-inject` invocation refused without `_NUDGE_PULSE_INTERNAL=1`. **MCP `chorus_nudge_message` is the only path from a Claude session.** Operational scripts (alerts, health checks) use `platform/scripts/ops-nudge` which POSTs to the same pulse endpoint.

## When to use /nudge vs other tools

| Need | Tool |
|------|------|
| Quick message, need response soon | **/nudge** |
| Detailed request with context | **Brief** (write to their `briefs/` dir) |
| Multi-role alignment needed | **/clearing** |
| Back-and-forth conversation | **/chat** |
| Jeff wants to direct a role | **/nudge** (from Jeff's current role) |

## Rules

- **Prefix your message with your role name** so the recipient knows who's talking
- **Keep it short** — 1-3 sentences. If you need more, write a brief
- **DEC-079**: Max 2 nudge exchanges per role pair per 30 min. If more back-and-forth, escalate to `/chat` or `/clearing`
- **Don't nudge for things that can wait** — if next session is fine, use a brief
- **Don't nudge yourself** — that's not how this works
