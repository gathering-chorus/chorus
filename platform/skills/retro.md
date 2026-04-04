# /retro — Team Retrospective

Surface patterns from recent work, connect each one to a mechanism that prevents recurrence. A retro that only names problems is an escalation tracker — the output must include gates.

## Arguments

```
/retro [topic]        — retro on a specific topic or theme
/retro [date-range]   — retro on a time period (e.g., "this week", "last 3 days")
/retro                — retro on the current session
```

## How to Execute

### Step 1: Gather signal

Pull from all available sources in parallel:

1. **Chorus logs** — `curl -s "http://localhost:3340/api/chorus/search?q=<relevant terms>"` for patterns, corrections, redirects
2. **Feedback memories** — scan `~/.claude/projects/-Users-jeffbridwell-CascadeProjects/memory/feedback_*.md` for related entries
3. **Session context** — what happened in this conversation (corrections, pivots, repeated friction)
4. **Board activity** — cards shipped, rejected, redirected via `board-ts`

### Step 2: Identify patterns

For each pattern found, classify it:

| Type | Signal |
|------|--------|
| **Belief** | Role assumed a constraint that wasn't real ("Claude bug", "this will take 30 min") |
| **Ceremony** | Unnecessary round-trip that costs Jeff attention ("should I?", "want me to?") |
| **Drift** | Mechanism exists but isn't being followed (feedback memory ignored, gate bypassed) |
| **Gap** | No mechanism exists — the failure can repeat freely |

### Step 3: Build the retro

For EACH pattern, produce exactly three things:

1. **What happened** — one sentence, with data (count, quote, date)
2. **Why it recurs** — the structural reason, not "we forgot"
3. **Gate** — the mechanism that would prevent it. One of:
   - **Hook** — automated block (like icd-gate-hook.sh)
   - **Feedback memory** — passive guidance for future sessions (already exists or needs creation)
   - **CLAUDE.md rule** — baked into role instructions
   - **Scheduled job** — cron/LaunchAgent that runs without session context
   - **None feasible** — some things require human judgment. Say so honestly.

### Step 4: Render as HTML

Output the retro as an HTML file to `/tmp/retro-<date>.html`. Include:

- Team name, date, topic
- Pattern table with the three columns (What / Why / Gate)
- Summary stats: patterns found, gates proposed, gaps remaining
- Any new cards created during the retro

Style: clean, printable, no frameworks. Jeff prints these and marks them up.

### Step 5: Act

For each pattern where a gate is feasible and doesn't exist yet:
- **Feedback memory missing?** Write it.
- **Card needed?** Create it with `board-ts add ... -q`.
- **Hook needed?** Card it for Silas.

Don't propose. Execute. The retro's value is measured by gates created, not patterns named.

### Step 6: Open the file

```bash
open /tmp/retro-<date>.html
```

## Rules

- Never present a pattern without a gate recommendation
- Never recommend a gate you can't justify with data from this retro
- "We should be more careful" is not a gate. Name the mechanism or say none exists.
- A retro that only surfaces problems is an escalation tracker. The Staples lesson: measure the fix, not the failure.
- Keep it tight — 3-7 patterns max. More than that means you're cataloging, not prioritizing.
