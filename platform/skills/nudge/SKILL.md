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

### Step 1: Detect your role

Use your working directory to determine your role name (wren, silas, kade).

### Step 2: Send the nudge

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge <target-role> "<your-role-name>: <message>"
```

The script will:
- Find the target role's active terminal TTY
- Inject the message via clipboard paste
- If the role is busy, queue it for delivery after their current turn
- If no active session found, queue to `/tmp/voice-inbox/<role>/pending.txt` for next session

### Step 3: Confirm delivery

The script outputs one of:
- `Injected to <role>` — delivered immediately
- `Injected to <role> (busy — will appear after current turn)` — queued for end of turn
- `Queued for <role> (no active session)` — saved for when they come online

Report the delivery status to Jeff.

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
- **DEC-079**: Max 2 nudge exchanges per role pair per 30 min. If you need more back-and-forth, escalate to `/chat` or `/clearing`
- **Don't nudge for things that can wait** — if next session is fine, use a brief
- **Don't nudge yourself** — that's not how this works
