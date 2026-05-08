---
name: chat
description: Lightweight two-role chat — direct terminal channel between roles. Jeff watches or participates.
user-invocable: true
---

# /chat — Lightweight Two-Role Chat

Direct channel between two roles in the terminal. No browser, no nudge latency. Jeff watches or participates.

## Arguments

```
/chat <role> [topic]
```

- `role` — silas, kade, or wren (the other party)
- `topic` (optional) — what the chat is about, defaults to "chat"

## How to Execute

### Step 1: Start the chat

```bash
CHAT_ID=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh start <your-role> <other-role> "<topic>")
```

Save the CHAT_ID — you need it for every message.

### Step 2: Write your opening message

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh say $CHAT_ID <your-role> "your message here"
```

### Step 3: Nudge the other role to join

```
mcp__chorus-api__chorus_nudge_message({
  to: "<other-role>",
  message: "[chat] Join chat $CHAT_ID — topic: <topic>. Read: bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh read $CHAT_ID | Reply: bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh say $CHAT_ID <their-role> \"message\" | When done, both stop."
})
```

### Step 4: Register cron tick (automatic — read the marker)

`chat.sh say` writes `/tmp/chorus-chat/tick-<CHAT_ID>` on first call. Read it and register the tick:

```bash
TICK_FILE="/tmp/chorus-chat/tick-${CHAT_ID}"
if [ -f "$TICK_FILE" ]; then
  LINE_COUNT=$(cut -d'|' -f1 "$TICK_FILE")
  OTHER=$(cut -d'|' -f2 "$TICK_FILE")
fi
```

Then register via CronCreate: `cron="*/1 * * * *"`, `prompt="/chat-tick <CHAT_ID> <OTHER_ROLE> <LINE_COUNT>"`, `recurring=true`. Save the job ID for cleanup.

**Do not sleep-poll.** The cron tick fires once per minute. Between ticks, do other work.

### Step 5: On each tick

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh read $CHAT_ID --since <last_line>
```

If new lines exist from the other role, read them, respond with `say`, update your line counter. Pass the updated line count in the next tick prompt.

### Step 6: End the chat

When done (topic resolved, Jeff says stop, or natural conclusion):

1. ```bash
   bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh end $CHAT_ID
   ```
2. CronDelete your tick loop (the tick marker is deleted by `chat.sh end`)
3. **Summarize for Jeff** — the role who ends the chat owns the close-out: one paragraph covering what was discussed, what was agreed, and what actions follow. This is the chat's permanent record.

## When receiving a chat invite (via nudge)

If you receive `[chat] Join chat <CHAT_ID>`:

1. Read the chat so far: `bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh read $CHAT_ID`
2. Reply: `bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh say $CHAT_ID <your-role> "response"`
3. Register cron tick (read `/tmp/chorus-chat/tick-<CHAT_ID>` for line count + other role, then CronCreate)
4. Continue the conversation until resolved

## Rules

- **Keep messages short** — 1-3 sentences per turn, like a real chat
- **Don't monologue** — write one message, wait for the reply
- **Jeff can see the transcript** — `bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chat.sh read $CHAT_ID`
- **Either party can end the chat** — `chat.sh end`
- **No clearing needed** — this is for two-role exchanges, not full-team alignment
- **Topic stays focused** — if scope creeps, end chat and card the new work
