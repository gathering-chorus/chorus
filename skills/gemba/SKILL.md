---
name: gemba
description: Live observation of a role working — one loom-gemba poll per look, narrate what it returns.
user-invocable: true
---

# /gemba — Live Role Observation

Watch a role work in real time. Sports commentary, not status reports.

## Arguments

```
/gemba <role>
```

## The mechanism (#3319)

The verb owns everything the model used to be able to skip. One MCP call:

```
mcp__chorus-api__loom-gemba({ role: "<your-role>", target: "<role>" })
```

`loom-gemba` is the observation verb built on the pulse-gather short-term-memory
core (one implementation of the cursor-exact gather mechanics — no turn lost, none
replayed, silence is true). Invoking it:

- **IS the declaration** — it sets `role-state <you> observing gemba=<target>`
  itself. There is no separate declare step to skip. A stale `observing` decays
  (role-state sweep, 10-min TTL) if you stop polling.
- **Returns the banner first, always** — `[gemba] you→target | since <cursor> |
  <n> new turns`, including `0 new turns (quiet)` and `stream unavailable —
  rebuilding (not idle)`. Then the fresh turns (`HH:MM:SS Tool — digest`).
- **Emits `gemba.observed`** on the spine.

## What YOU do (the only model-side work)

1. Call the MCP tool.
2. **Paste the returned text verbatim in your reply** — focus mode: Jeff sees only
   your typed message; the banner is his visibility into the watch.
3. Narrate: 2-3 sentences on what the role is doing and how it's going. Only what
   the output shows — never improvise activity. Quiet poll = say it's quiet.
4. To keep watching, re-invoke. **This is a poll you RE-RUN, not a maintained
   loop** — no cron, no background watcher. If you stop re-polling, you've
   stopped observing (the TTL will tell the truth for you).

## Commentary format

```
[HH:MM] <what happened>. <what it means>. <flag if any>.
```

2-3 lines max. Sports announcer energy.

**Anti-patterns:** don't dump raw turn-lines without the banner; don't intervene in
the observed role's session; don't launch explore agents — the verb gives you the
stream.

## Exit

Triggers: Jeff says stop, card accepted/rejected, or Jeff moves to other work.

1. Debrief: one paragraph — what the role accomplished across the watch.
2. `role-state <you> waiting` — or just stop polling; the decay closes the state
   honestly either way.
