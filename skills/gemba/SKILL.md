---
name: gemba
description: Live observation of a role working — poll pulse-gather + spine events, discuss what you notice with Jeff.
user-invocable: true
---

# /gemba — Live Role Observation

Watch a role work in real time. Sports commentary, not status reports.

## Arguments

```
/gemba <role> [card-id]
```

## How it works (#3205, #3274)

Gemba is one poll of the `pulse-gather` verb. There is no start script, no tick
script, no snapshot-diff layer — those caused the staleness Jeff complained about
("no change since 16:56" while the role's real last turn was newer). `pulse-gather`
reads the role's live observation stream and emits every turn newer than its cursor,
keyed on timestamp, so a turn between polls is never lost and never replayed.

**Cold start is windowed (#3274):** a /gemba with no prior cursor shows the role's
last ~10 turns (current activity), NOT the whole backlog dumped back to yesterday.
Re-polls stay exact (every turn since your last poll).

**This is a poll you RE-RUN, not a maintained loop (#3274).** /gemba does not create a
cron or a background watcher — there is no continuous observer running on your behalf.
To keep watching, you re-invoke the poll. The skill claims only what it does: a
windowed poll, on demand.

**Keep: the poll + narrate.** That's the whole skill.

## How to Execute

### Step 1: Declare state + first poll

```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/role-state <your-role> observing gemba=${ROLE}
pulse-gather ${ROLE}
```

`pulse-gather` prints the role's fresh turns (`HH:MM:SS Tool — digest`), advances its
own cursor, and emits a `pulse.gathered` spine event. On genuine no-change it prints
nothing — that silence is real (cursor is at the newest turn), not stale.

Read the output. Print your read — 2-3 sentences: what the role is doing, how it's
going. That's it.

### Step 2: Re-poll to keep watching (no cron — #3274)

To follow the role, RE-RUN the poll yourself — there is no maintained loop:

```bash
pulse-gather ${ROLE}
```

Each re-run commentates only the deltas since your last poll (the cursor is durable —
off /tmp, survives reboot — so a turn is never re-narrated or lost). #3274 retired the
`*/1` cron loop this step used to document: nothing created or maintained it, so
it was a false claim of continuous observation (the same cron-fires-skill pattern
#3253 retired). If you stop re-polling, you've stopped observing — set your role-state
to match (Exit), never leave a stale `observing`.

### Step 3: Commentary format

```
[HH:MM] <what happened>. <what it means>. <flag if any>.
```

2-3 lines max. Sports announcer energy. Only report what `pulse-gather` outputs — don't
improvise data. If the poll is empty, the role genuinely hasn't acted since the last
tick; say so plainly, don't manufacture activity.

**Anti-patterns:**
- Don't dump raw output at Jeff
- Don't intervene in the observed role's session
- Don't launch explore agents or read artifacts — the verb gives you the stream

### Exit

Triggers: Jeff says stop, card accepted/rejected, or Jeff moves to other work. In
/pair sessions, gemba is scope-boxed to the pair duration.

Exit sequence:
1. Debrief: one paragraph — what the role accomplished across the session
2. `role-state <your-role> waiting` — you've stopped observing, so the state reflects reality (never leave a stale `observing`; role-state is a live marker, not activity truth — #3274)
