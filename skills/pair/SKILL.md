---
name: pair
description: Start a pair session — strong-style pairing with navigator scope loop, work-mode detection, and rotation checkpoints.
user-invocable: true
---

# /pair — Strong-Style Pair Session

Two roles working one problem. The **navigator's scope loop** is the pair. When the loop stops, the pair ends.

**Research foundation:** Williams & Kessler, Falco (strong-style), Zuill (mob), Plonka (disengagement). Full reference: `data/about/PAIR_PROGRAMMING_RESEARCH.md`

## The Navigator's Scope Loop (THE CORE PATTERN)

**The pair lives as long as this loop turns. When it stops, the pair is over.**

```
┌─────────────────────────────────────────┐
│  1. CHECK AC — what's unchecked?        │
│  2. DIRECT driver — do this next        │
│  3. MONITOR progress — is it working?   │
│  4. INVESTIGATE — what's the next gap?  │
│  5. → back to 1                         │
└─────────────────────────────────────────┘
```

**Rules of the loop:**
- **Step 3 (MONITOR) is one poll of `pulse-gather <driver-role>` (#3205)** — the same verb gemba uses, no second awareness path. It emits the driver's fresh tool-turns since the last poll (keyed on timestamp, so nothing is missed between loop turns and nothing replays). Read its output to see what the driver actually did; never infer progress from silence.
- The navigator holds the **card's AC**, not their own contribution. When the navigator finishes *their* work, they check the AC — not stop.
- The navigator reports to the **driver**, not to Jeff. Jeff gets the gemba summary from the PM. Reporting to Jeff is leaving the pair to seek validation.
- A blocking operation (long query, batch run) is the loop turning slowly, not the loop stopped. The navigator should check progress, not wait.
- If the navigator goes silent > 2 minutes, the driver asks: "Navigator, what's unchecked on the AC?"
- If the navigator has no new directive after completing a task, the navigator re-reads the AC and finds the next gap.

**What kills the loop:**
- Reporting to Jeff ("Jeff — active pair status...") — the dopamine of "I updated the boss" replaces the drive to continue
- Tailing without commentary — watching is not attending
- Declaring personal done — "my inserts are complete" without checking what the card still needs
- Confirmation bias — "I reported a number, the number is good, I'm done"

## Arguments

```
/pair <role> [card-id]
```

- `role` — the role to pair with (silas, kade, or wren)
- `card-id` (optional) — the card to work on. If omitted, uses current WIP card.

## Step 0: Card setup — delegate to /pull

**If the card is not already in WIP, run `/pull <card-id>` first.** /pull owns all engineering gates (validate, preflight, WIP check, domain context, TDD readiness). /pair adds the collaboration protocol — not separate engineering gates.

```bash
CARD_VIEW=$(bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view <card-id> 2>&1)
STATUS=$(echo "$CARD_VIEW" | grep -oE 'Status:\s+\w+' | awk '{print $2}')
if [ "$STATUS" != "WIP" ]; then
  # Card not in WIP — run /pull to pass engineering gates and move it
  echo "Card not in WIP — running /pull <card-id> first."
  # Invoke /pull skill — all gates run, card moves to WIP
fi
```

If the card IS already in WIP (pulled earlier or by another role), skip to Step 1.

## Step 1: Establish the pair

1. **Determine roles**: The invoking role is the **driver**. The target role is the **navigator**.

2. **Declare state for both roles** (#2467: card lives on the board, not in role-state):
   ```bash
   /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/role-state <your-role> building
   ```

3. **Emit spine event**:
   ```bash
   /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pair.started <your-role> card=<card-id> partner=<target-role>
   ```

4. **Nudge the navigator to load /pair** — the navigator MUST load the full skill, not just follow a text description:
   ```
   mcp__chorus-api__chorus_nudge_message({
     to: "<target-role>",
     message: "/pair <card-id> <your-role> — You navigate, <your-role> drives. Load this skill now so you have the full protocol."
   })
   ```
   The `/pair` prefix in the message triggers the navigator's system to load the skill. Both roles must have the full protocol loaded — a text summary is not sufficient.

5. **Navigator opens the shared scratch file** — read `/tmp/pair-<card-id>.md` at session start.

6. **Set rotation checkpoint** — note the start time. At 15 minutes, both roles evaluate: swap, re-commit, or shift work mode.

## Step 2: Shared context setup

1. **Card context** — both roles read the card:
   ```bash
   bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards view <card-id>
   ```

2. **Shared scratch file** — create `/tmp/pair-<card-id>.md`:
   - Card title, AC checklist (copy verbatim — this is the navigator's loop target)
   - Known blockers, machine topology
   - `## Navigator Directions` — what the navigator told the driver to do
   - `## Navigator Fixes` — what was prevented vs just observed
   - `## Work Mode Log` — track mode transitions
   - `## Follow-on Cards`
   - `## For Jeff` (filled at pair-end)

3. **Determine initial work mode** (see Step 3).

4. **Print the pair contract** — one line to Jeff:
   ```
   Pair started: <driver> (drives) + <navigator> (scope loop) on #<card-id>. Mode: <build|investigate>.
   ```

## Step 3: Work modes

**Detect and declare the work mode.** Different work needs different pair dynamics. Log mode transitions in the scratch file.

### Mode: BUILD (driver/navigator, strong-style)

Use when: writing code, running scripts, executing commands, deploying.

**Navigator behavior — the scope loop in BUILD mode:**
1. **CHECK AC** — which items are unchecked? Pick the next one.
2. **VERIFY DATA** — before directing work based on pipeline output, verify claims against source of truth. Pipeline data is a hypothesis, not a fact. "22K records have no imagePath" → check the filesystem. "No thumbnails" → count what's on disk. If the domain has an ICD (e.g., `SEMANTIC_MAPPER.html`), check the contract tiers — Required fields must be 100%, not "improved." Never scope fixes based on ungated data. Never exclude records to hide pipeline problems — if it exists at source, it exists in canonical.
3. **DIRECT the driver** — based on verified data, not pipeline assertions. "Query Fuseki for the photo count." "Use SSH not NFS — DEC-089." "Smoke test before full batch."
4. **MONITOR progress** — is the driver executing? Is it working? Are counts changing?
5. **INVESTIGATE the next gap** — while the driver executes, start researching what the next AC item needs. Don't wait for the driver to finish before thinking ahead.
6. **REPEAT** — back to CHECK AC.

**Navigator anti-patterns:**
- Reporting to Jeff mid-pair → the navigator's audience is the driver
- Tailing without commentary → watching is not attending
- "My part is done" → check the AC, not your task list
- Single tail check then stop → set up a recurring check or query repeatedly
- Silence after reporting a number → the number is a finding, not a stopping point
- Scoping work from pipeline output without verifying → "22K have no imagePath" may mean bad mapping, not missing files. Check the filesystem before treating a gap as real.
- Excluding records to shrink the problem → if the source has it, the canonical must account for it. Fix the integration, don't hide the gap.

**Driver behavior:**
- **Narrate intent before executing.** "I'm going to rsync the thumbnails from Bedroom." This gives the navigator a chance to redirect.
- **When you hit a wall outside your domain:** say it, don't solve it. "Hit SSH auth failure — navigator's domain."
- **If no navigator directive for 2 minutes:** ask "Navigator, what's unchecked on the AC?"
- **Write findings to scratch file** when something non-obvious surfaces.

### Mode: INVESTIGATE (split-and-reconvene)

Use when: understanding a problem, researching data, querying systems, reading code. **Research shows browsing together on one screen is unproductive.**

**Protocol:**
1. **Define the question** together: "How many real photos do we have?"
2. **Split the investigation** by angle:
   - Role A: pipeline side (code, TTL files, app queries)
   - Role B: data side (graph queries, filesystem counts, source verification)
3. **Timebox: 5 minutes.** Both work independently.
4. **Reconvene:** Compare findings in the scratch file. Resolve discrepancies. Decide next action.
5. **Repeat or transition to BUILD.**

**The scope loop still applies in INVESTIGATE mode.** Each role checks the AC to know what question to investigate. The reconvene is where the loop's CHECK step happens jointly.

### Mode: SIMPLE (solo with review)

Use when: well-understood task, single domain, low complexity.

**Protocol:** One role works. The other does different productive work. Review the output when done. **Don't pair on simple tasks — research shows it wastes effort without quality gain.**

## Step 4: Rotation checkpoints (every 15 minutes)

At each checkpoint, both roles evaluate:

1. **Swap?** Has the work shifted to the navigator's domain? Swap driver/navigator.
2. **Mode shift?** Has the work shifted from build to investigate (or vice versa)? Declare the new mode.
3. **Re-commit?** Current arrangement working? Say so explicitly and continue.
4. **Energy check:** Is either role's context getting full? Consider ending the pair.
5. **Loop check:** Is the navigator's scope loop still turning? If the navigator has been quiet or only monitoring, that's a signal to swap or shift.

Emit on swap:
```bash
chorus-log pair.swapped <role> card=<card-id>
```

**The rotation checkpoint doesn't always mean swap.** Sometimes it means "confirm the split is working and both roles are active." A domain-based split (Kade on code, Silas on graph) can sustain longer than 15 minutes if both roles are producing.

## Step 5: Cascade prevention

The highest-value moment in BUILD mode is when the navigator prevents a cascade:

```
Driver: "Restarted the app via app-state.sh"
Navigator sees: same PID, Fuseki didn't actually restart
Navigator: "Fuseki PID unchanged — needs launchctl kickstart, not app-state.sh restart"
→ Saved: 20 minutes of debugging
```

```
Driver: starts `find` across NFS-mounted directory
Navigator: "That data is on Bedroom — SSH and run locally. DEC-089."
→ Saved: 10 minutes of NFS crawl
```

```
Driver: long-running migration at 1%/hr
Navigator: "Kill it. Batched version at /tmp/migrate-batch.py. Data is safe — I verified."
→ Saved: hours of waiting
```

The navigator's job is to catch the 30-second mistake before it becomes a 30-minute investigation.

## Step 6: Outcome gate (before closing)

**Before declaring "done," validate against the outcome, not the task.**

The builder's instinct is to close when the AC is met. The pair's job is to ask: **"Did the thing we did change what Jeff sees?"**

Checklist before close:
- [ ] Does the user-visible result match reality? (Check the page, not the terminal.)
- [ ] Does the count match an external source of truth? (Google Dashboard, filesystem, not just Fuseki.)
- [ ] Would Jeff look at this and say "that's right"?

If any answer is no, the card stays open. Don't declare victory against internal metrics.

## Step 7: End the pair

**Triggers:**
- Card work is complete AND outcome gate passes
- Jeff says "unpair" or redirects to different work
- Natural break — one role needs to reboot or context is full
- Work is no longer cross-domain (driver can solo from here)
- 45-minute cognitive ceiling reached — take a break or end
- **The navigator's scope loop has stopped and can't be restarted** — both roles blocked on the same long-running operation with nothing else to investigate

**Sequence:**
1. Stop any background tail tasks
2. Write session summary to the shared scratch file
3. Emit spine event:
   ```bash
   /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log pair.ended <your-role> card=<card-id> partner=<target-role> duration=<minutes>
   ```
4. Declare state back:
   ```bash
   /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/role-state <your-role> waiting
   ```
5. **Write the Jeff summary** in the scratch file — required, not optional:
   ```
   ## For Jeff
   - Input: what entered the pipeline
   - Filtered: what was excluded and why
   - Output: what changed in Jeff's data
   - Data impact: collection sizes before → after
   - Integrity: provenance tags, source tracking, dedup status
   - What to verify: specific URL + filter to check the result
   - Outcome validated: yes/no + how
   ```

6. One-line summary to Jeff:
   ```
   Pair ended: <driver> + <navigator> on #<card-id>, <duration> min. <what was accomplished>.
   ```

## When to pair vs. other mechanisms

| Situation | Use |
|-----------|-----|
| Quick question, one-shot answer | `/nudge` |
| Sustained discussion, no shared work | `/chat` |
| Observe a role working, no co-action | `/gemba` |
| All three roles need alignment | `/clearing` |
| Simple single-domain task | Solo with review |
| **Cross-domain card, cascade risk, both roles need to act** | **`/pair`** |

**The test:** "Will this card require more than 2 nudge round-trips between these roles?" If yes, pair.

## DND Mode — Pair Isolation (DEC from 2026-03-16 session)

**Problem:** Any intrusion breaks the pair. Nudges from Wren, messages from Jeff, the navigator turning to report — all kill the scope loop. Agents treat every incoming message as equal priority and can't say "hang on."

**Three failure modes observed 2026-03-16:**
1. Wren dropped gemba to pair directly with Kade — killed the observation loop
2. Silas navigated correctly, then announced to Jeff and went idle — broke his own loop
3. Wren asked Jeff about gemba TTL instead of continuing scope-boxed — permission-seeking

**DND protocol:**
1. When pair starts, both roles enter DND. External nudges **queue to the shared scratch file** (`## Queued Messages`) instead of injecting into the session.
2. The navigator's heartbeat tick (every 60s) reads the scratch file for new entries in `## Queued Messages` and `## PM Notes`. Process them within the pair context — don't leave the pair to respond.
3. If Jeff messages during active pair, auto-acknowledge: "pair active on #N, will respond at checkpoint."
4. Only these break through DND: explicit `/pair stop`, `card.accepted`, or Jeff saying "stop/unpair/enough."
5. When Wren gemba's a pair, Wren writes to the scratch file — not nudge. The scratch file is the shared channel.

**Heartbeat tick (navigator):**
```
Every 60 seconds:
  1. Read /tmp/pair-<card-id>.md for new ## Queued Messages or ## PM Notes
  2. Process any new entries (steer driver if relevant)
  3. Check driver's tail for progress
  4. Loop back to scope loop step 1 (CHECK AC)
```

**Navigator attention monitor (driver-side, MANDATORY — #1897):**

The driver sets up a cron tick at pair start that monitors for navigator silence. This is the enforcement mechanism — the navigator's heartbeat is self-reported, this one is observed.

**Setup (driver does this in Step 1):**
```bash
# Record pair start + last navigator activity timestamp
echo "$(date +%s)" > /tmp/pair-nav-last-activity-<card-id>

# CronCreate: cron="*/1 * * * *", prompt="/pair-heartbeat-check <card-id> <navigator-role>"
```

**On each tick, the driver checks:**
```bash
CARD_ID=<card-id>
NAV_ROLE=<navigator-role>
SCRATCH="/tmp/pair-${CARD_ID}.md"
LAST_FILE="/tmp/pair-nav-last-activity-${CARD_ID}"
NOW=$(date +%s)
LAST=$(cat "$LAST_FILE" 2>/dev/null || echo "$NOW")
ELAPSED=$(( NOW - LAST ))

# Check for navigator activity: nudges received, scratch file changes, chat messages
SCRATCH_MOD=$(stat -f %m "$SCRATCH" 2>/dev/null || echo "0")
if [ "$SCRATCH_MOD" -gt "$LAST" ]; then
  echo "$SCRATCH_MOD" > "$LAST_FILE"
  ELAPSED=0
fi
```

**Stall signals (advisory, not blocking):**
- **60s silence:** Print: `⚠ Navigator silent for 60s — pair may be stalling. Check: is ${NAV_ROLE} thinking, blocked, or disengaged?`
- **120s silence:** Re-nudge navigator: `[pair] Navigator silent 2min on #${CARD_ID} — are you blocked or did you lose context?`
- **180s silence:** Emit spine event + escalate: `pair.navigator.stall card=${CARD_ID} elapsed=180s`. Write to scratch `## Escalation`. Nudge Wren for gemba.

**Reset triggers — any of these reset the 60s clock:**
- Navigator nudge received
- Scratch file modified (navigator wrote directions)
- Chat message from navigator
- Navigator's cron tick fires (proves navigator session is alive)

**Cleanup (driver does this at pair end):**
```bash
# CronDelete the heartbeat check
rm -f /tmp/pair-nav-last-activity-<card-id>
```

**Heartbeat escalation (driver completion):**
When the driver completes an AC item and nudges the navigator:
- **0-60s:** Navigator responds with next directive. Normal flow.
- **60-120s:** Stall warning prints automatically (from heartbeat monitor above).
- **120s+:** Driver re-nudges with urgency (automatic from heartbeat monitor).
- **180s+:** Spine event + Wren escalation (automatic from heartbeat monitor).

**Why escalate at 3 minutes, not 10?** Because 10 minutes of silence in a pair is the pair dying. The navigator's scope loop IS the pair — if it stops for 3 minutes without a declared reason (blocking operation, investigate mode), the pair is already over. Early escalation gives the navigator a chance to re-engage before the session context drifts.

**Shared scratch file as live channel:**
- `## PM Notes` — Wren writes product observations here during gemba
- `## Queued Messages` — nudges from outside the pair land here
- `## Navigator Directions` — navigator's steering record
- Navigator reads these sections on heartbeat, not on interrupt

## Rules

- Either role or Jeff can invoke `/pair`
- **The navigator's scope loop IS the pair.** When the loop stops, the pair ends.
- **Navigator reports to the driver, not Jeff.** PM reports to Jeff via gemba.
- **Navigator holds the card's AC, not their own contribution.** Personal done ≠ card done.
- **Driver pulls direction after 2 min navigator silence:** "Navigator, what's unchecked on the AC?"
- **DND mode is automatic on pair start.** External messages queue, not inject.
- Rotation checkpoint every 15 minutes — mandatory, not optional
- Work mode must be declared and logged on transitions
- Outcome gate before closing — validate against reality, not internal metrics
- A blocking operation with intent is the loop turning slowly, not stopped
- **Never blame browser cache.** Diagnose the real cause.
- **Never use Jeff as a remote keyboard.** Diagnose or park.
- Shared scratch file is ephemeral — survives the session, not the week
- **Shared scratch file is a live channel** — not just setup notes. Heartbeat reads it.
- Jeff can `/gemba` a pair — he sees both roles working. He writes to the scratch file, not nudge.
- DEC-091 applies — both roles card, schedule, and chore without asking
- Maximum pair duration: context window or 45 minutes, whichever is shorter. Then break or end.
