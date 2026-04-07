---
name: gemba
description: Live observation of a role working — tail session + spine events, discuss what you notice with Jeff.
user-invocable: true
---

# /gemba — Live Role Observation

Watch a role work in real time. Sports commentary, not status reports.

## Arguments

```
/gemba <role> [card-id]
```

## How to Execute

### Step 1: Run gemba-start.sh (< 5 seconds)

```bash
bash /Users/jeffbridwell/CascadeProjects/platform/scripts/gemba-start.sh ${ROLE}
```

Read the output. Print your read — 2-3 sentences: what the role is doing, how it's going. That's it.

Declare state:
```bash
/Users/jeffbridwell/CascadeProjects/platform/scripts/role-state <your-role> observing gemba=${ROLE}
```

### Step 2: Start the cron loop

```bash
START_EPOCH=$(date +%s)
# CronCreate: cron="*/1 * * * *", prompt="/gemba-tick <ROLE> <START_EPOCH>", recurring=true
```

### Step 3: Commentary format

On each tick, read the script output and commentate in this format:

```
[HH:MM] <what happened>. <what it means>. <flag if any>.
```

2-3 lines max. Sports announcer energy. Don't say "quiet" unless the script confirms no activity. Don't improvise data — only report what the script outputs.

**Anti-patterns:**
- Don't say "quiet" when team-scan is stale — stale ≠ idle
- Don't dump raw output at Jeff
- Don't intervene in the observed role's session
- Don't launch explore agents or read artifacts — the script gives you everything

### Exit

Triggers: Jeff says stop, card accepted/rejected, 10-minute TTL (computed from epoch, never estimated), or Jeff moves to other work. In /pair sessions, TTL is scope-boxed to the pair duration.

Exit sequence:
1. CronDelete the loop
2. Compute elapsed: `echo $(( $(date +%s) - START_EPOCH ))`
3. Debrief: one paragraph, include elapsed seconds
4. `role-state <your-role> waiting`
