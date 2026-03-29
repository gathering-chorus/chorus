---
name: chorus
description: Query the shared memory index — Slack conversations + Claude sessions + artifacts. Use to reconcile context, search team history, or check what a specific role has been doing.
---

# Chorus — Shared Memory Query

This skill queries the Chorus context index, which contains indexed Slack conversations and Claude session transcripts across all three roles (Wren, Silas, Kade).

## Reconcile (default — what happened while I was offline?)

Run this to catch up on what happened since your last session:

```bash
bash ~/.chorus/scripts/chorus-query.sh reconcile
```

This will:
1. Incrementally update the index (fetch new Slack messages + scan new session transcripts)
2. Find your last session end timestamp
3. Show all Slack messages, other roles' Claude sessions, and artifact changes since then

## Search

To search across all indexed content (both Slack and Claude sessions):

```bash
bash ~/.chorus/scripts/chorus-query.sh search "<term>"
```

Replace `<term>` with your search query. Examples:
- `search cockpit` — find all discussions about the cockpit
- `search DEC-028` — find all references to a specific decision
- `search "Photos harvester"` — find discussions about Photos

## Role Activity

To see what a specific role has been doing:

```bash
bash ~/.chorus/scripts/chorus-query.sh role <name>
```

Where `<name>` is: wren, silas, kade, or jeff

## Stats

To see index statistics:

```bash
bash ~/.chorus/scripts/chorus-query.sh stats
```

## Tail

Show recent activity from another role's active session — like looking over their shoulder.

### One-shot (default)

```bash
bash ~/.chorus/scripts/chorus-query.sh tail <role> [--lines N]
```

Prints the last N entries (default 10) from the role's most recent session, then exits.

### Follow mode (sticky screen share)

Follow mode keeps watching. New activity surfaces incrementally while you continue working.

**How to use it as a role:**

1. Start the tail in the background:
   ```bash
   bash ~/.chorus/scripts/chorus-query.sh tail <role> --follow
   ```
   Run this with `run_in_background: true`. The first output line after the snapshot contains `PID:<number>` — note the background task ID.

2. Show the initial snapshot to Jeff, then continue your conversation normally.

3. **On every subsequent response**: check the background task output for new `---batch:N---` sections. If new entries appeared, surface them under a compact header like `### <role> activity (batch 3)`. If nothing new, say nothing about the tail.

4. **Never auto-terminate.** Only stop when Jeff says to stop watching.

5. Don't analyze or comment on the tailed activity unless Jeff asks. Just surface it.

6. **To stop**: use TaskStop on the background task ID. Print a brief summary: how long the tail ran and how many batches surfaced.

## How to Use Results

After running a query, analyze the results and:
1. **Identify stale state** — if another role made decisions or shipped work that affects your state files, flag it
2. **Surface Jeff's direction** — any messages from Jeff in other sessions that this role hasn't seen
3. **Connect dots** — link related discussions across planes (e.g., a Slack conversation that led to a Claude session decision)
4. **Recommend actions** — based on what happened, what should this role do next?
