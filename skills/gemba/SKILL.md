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

## How it works (#3205)

Gemba is one poll of the `pulse-gather` verb. There is no start script, no tick
script, no snapshot-diff layer — those caused the staleness Jeff complained about
("no change since 16:56" while the role's real last turn was newer). `pulse-gather`
reads the role's live observation stream and emits every turn newer than its cursor,
keyed on timestamp, so a turn between polls is never lost and never replayed.

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

### Step 2: Loop the poll

```bash
# CronCreate: cron="*/1 * * * *", prompt="/gemba ${ROLE}", recurring=true
```

Each firing re-runs `pulse-gather ${ROLE}` and commentates the deltas. Because the
cursor is durable (off /tmp, survives reboot), the loop never re-narrates a turn it
already showed.

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
1. CronDelete the loop
2. Debrief: one paragraph — what the role accomplished across the session
3. `role-state <your-role> waiting`
