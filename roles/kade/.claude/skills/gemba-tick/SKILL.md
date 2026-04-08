# /gemba-tick — Gemba Observation Loop Tick

Internal skill — invoked by cron, not by Jeff directly.

## Arguments

```
ROLE=<first argument>
```

## How to Execute

### Step 1: Run gemba-tick.sh

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/gemba-tick.sh ${ROLE}
```

### Step 2: Read the output and commentate

Format:
```
[HH:MM] <what happened>. <what it means>. <flag if any>.
```

2-3 lines max. Only report what the script outputs. Don't improvise.

If script says "(no new indexed activity)" AND team-scan shows recent tool calls: say what team-scan shows. Don't say "quiet."

If script says "TTL EXPIRED": run exit sequence.

### Step 3: Exit sequence (if triggered)

1. CronDelete the loop
2. Debrief: one paragraph, include elapsed time from script output
3. `role-state <your-role> waiting`

## Rules

- Only report what the script gives you — no guessing
- Never say "quiet" without evidence of actual inactivity
- Never intervene in the observed role's session
- Match the chorus prompt format on every response
