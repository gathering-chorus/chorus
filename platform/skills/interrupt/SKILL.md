# /interrupt — Break Into a Role's Session

Stop what a role is doing and redirect. Esc cancels the current generation, then a nudge delivers the new direction.

## Arguments

```
/interrupt <role> <message>
```

- `role` — silas, kade, or wren
- `message` — the redirect direction (what to do instead)

## How to Execute

### Step 1: Send Escape to cancel current generation

```bash
bash /Users/jeffbridwell/CascadeProjects/platform/scripts/nudge <role> "\x1b" --force
```

This sends an Escape character via osascript to the target role's terminal, canceling whatever Claude is currently generating.

### Step 2: Wait briefly for cancellation

```bash
sleep 1
```

### Step 3: Send the redirect as a nudge

```bash
bash /Users/jeffbridwell/CascadeProjects/platform/scripts/nudge <role> "<message>" --force
```

The role's terminal is now idle (generation cancelled) and receives the new direction immediately.

## When to Use

- A role is building the wrong thing — stop before more damage
- Scope drift mid-card — redirect back to agreed plan
- Wrong-role handoff in progress — cancel before the target loads context
- Jeff says "stop" from Bridge — relay as interrupt to the target role

## Rules

- Always send the redirect message, not just the Esc. Cancelling without redirecting leaves the role confused.
- The message should be direction, not explanation. "Build X instead" not "you were wrong because..."
- Log the interrupt: `chorus-log role.interrupted <your-role> target=<role> reason=<brief>`
