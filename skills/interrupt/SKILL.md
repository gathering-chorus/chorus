# /interrupt — Break Into a Role's Session

> **Known gap (#2814):** Step 1's Escape-keystroke injection used `bash nudge "\x1b" --force` to send a raw control character via direct osascript. Post-#2804/#2809 the bash path is gone and the MCP `chorus_nudge_message` interface treats the message as text only — sending `"\x1b"` types the literal four characters `\x1b`, not an Escape key. `/interrupt` therefore cannot cancel a running generation today; only Step 3's redirect message works. Filed: see follow-on for an interrupt-specific primitive (Wren).

Stop what a role is doing and redirect. Today this skill only delivers the redirect — the cancellation step is broken pending the follow-on.

## Arguments

```
/interrupt <role> <message>
```

- `role` — silas, kade, or wren
- `message` — the redirect direction (what to do instead)

## How to Execute (current, partial)

### Step 1: Send the redirect as a nudge

```
mcp__chorus-api__chorus_nudge_message({
  to: "<role>",
  message: "<your-role-name>: [INTERRUPT] <message>"
})
```

The recipient sees the redirect after their current turn completes. **They are NOT cancelled mid-generation** — that capability is gone until the follow-on lands.

### Step 2: Log the interrupt

```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log role.interrupted <your-role> target=<role> reason=<brief>
```

## When to Use

- A role is building the wrong thing — stop before more damage (delivery is best-effort post-turn until follow-on)
- Scope drift mid-card — redirect back to agreed plan
- Wrong-role handoff in progress — cancel before the target loads context
- Jeff says "stop" from Bridge — relay as interrupt to the target role

## Rules

- Always include direction, not just "stop." The recipient needs to know what to do instead.
- The message should be direction, not explanation. "Build X instead" not "you were wrong because..."
