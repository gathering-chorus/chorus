---
name: pipeline
description: Orchestrate a multi-card pipeline — Wren drives, roles nudge back on completion, Jeff sees the demo at the end.
user-invocable: true
---

# /pipeline — Orchestrate Multi-Card Pipeline

Run a sequence of cards using board dependency chains (#1636). No YAML manifests — the board IS the pipeline.

## Arguments

```
/pipeline <action> [args]
```

Actions:
- `create <name> <card-ids...>` — wire cards into a dependency chain
- `status <root-card-id>` — show chain progress
- `advance` — check all chains, nudge ready steps
- `list` — show all active chains

## How to Execute

### Create a Pipeline

```
/pipeline create "Board API redesign" 1635 1636 1637 1638
```

1. Wire dependencies using `board set`:
   ```bash
   board set 1636 after=1635
   board set 1637 after=1636
   board set 1638 after=1637
   ```
2. First card in chain has no `after=` — it's the root
3. Comment pipeline name on each card
4. Move root card to Now, nudge owner
5. Print the chain: `board chain <root-id>`

### Status Check

```
/pipeline status 1635
```

Runs `board chain 1635` which shows the full sequence with per-card status:

```
Chain from #1635:
✅ #1635 board set — unified mutation verb
  🔨 #1636 board deps — sequencing (after #1635)
    ⬜ #1637 Migrate hooks + skills (after #1636)
      ⬜ #1638 Retire old verbs (after #1637)

2/4 complete
```

### Advance

```
/pipeline advance
```

1. Run `board ready` — shows all cards whose deps are Done
2. For each ready card: check AC exists, nudge owner to pull
3. Pipeline complete when `board chain <root>` shows all Done

### Responding to Completion

When a card ships (`board done`), the auto-unblock in #1636 moves gated cards Later→Next. `board ready` surfaces them. Wren nudges the next builder.

**No cron loops needed.** The board does the state management. `board ready` on every Wren prompt cycle catches completions.

### Re-Nudge on Silence

If `board ready` shows the same cards for multiple cycles:
- 15 min: gentle reminder
- 30 min: status check
- 1h+: "are you blocked?"
- 4h+ during working hours: escalate to Jeff

### Pipeline Nudge Protocol

Outgoing from Wren:
- `[pipeline] #<card> is ready — deps met. Pull and build.`
- `[pipeline] <name> complete. <N> cards shipped. Demo ready.`

Expected back from roles:
- `[done] #<card> — <one-line summary>`

## Rules

- Wren is always the orchestrator
- Dependencies live in the board (Vikunja task relations) — no YAML manifests
- `board chain` is the single source of truth for pipeline state
- `board ready` is the advance mechanism
- Parallel steps (no shared dep) get nudged simultaneously
- Auto-advance on card completion (built into board done)
- Jeff gets notified at chain completion, not per-card
- Won't Do cards in a chain are flagged as gaps by `board chain`
