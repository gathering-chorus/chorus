# One trace across both delivery surfaces

**Jeff, 2026-08-13 19:14:** "we should be able to correlate delivery to both claude code and clearing."

## Why now

At 19:11:54 a message he sent Kade was dropped in transport:

```
jeff.input.failed  bridge → kade  chars=23  reason="This operation was aborted"
```

Not hidden, not folded — gone. He had no way to know. Today's render defects (#3862) were about messages the room *had* and refused to draw; this is a different failure, upstream of anything the Clearing can fix.

## The actual gap

The events already exist. In the last 400 spine lines:

```
jeff.input.delivered   142
jeff.input.surfaced    145
jeff.input.failed        2
```

Those numbers don't reconcile, and **nothing can reconcile them**, because a delivery event carries only:

```
role, to, chars, timestamp, component, product, value_stream
```

No message id. So "142 delivered, 145 surfaced" is the most precise statement the system can make. Which three, and whether any of them was Jeff's, is unanswerable — the delta is visible but never nameable. That is the shape of every "I sent something and nothing happened" report: a real gap with nothing to point at.

## The change

One id, minted at the bridge when a message enters, carried unchanged through every leg.

```
bridge     mints  msg_id
           emits  jeff.input.delivered  {msg_id, to, surface: claude-code}
clearing   emits  message.ingested      {msg_id, surface: clearing}
pulse      emits  jeff.input.surfaced   {msg_id, to}
```

A swallowed message then becomes a **query**, not an intuition:

```
delivered WHERE NOT EXISTS ingested(msg_id)
```

Two properties matter more than the schema:

- **Both surfaces, one id.** Claude Code and the Clearing are delivery legs of the same message. Today they are separate facts with nothing joining them, which is why "did Kade see it, and is it in the room?" gets answered by guessing.
- **A missing leg is loud.** The correlation must go red when a leg is absent. A check that only compares totals reproduces exactly the situation above — a visible delta, no name attached.

## What this is not

It is not a fix for the 19:11 abort. That is a transport-retry question and belongs on its own card. This is the instrument that would have reported the abort *at the time*, instead of my finding it in a log an hour later because I happened to grep.

## Negative proof (#3734)

Ships with a fixture that drops a message between legs and shows the correlation naming it. A check that passes on a complete trace and stays green on a broken one is the defect this brief exists to end — and it is the shape I shipped twice today before catching it.

## Status

Shaped, not started. No card; Jeff said no action needed tonight.
