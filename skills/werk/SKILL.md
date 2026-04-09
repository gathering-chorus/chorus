# /werk — Work State Dashboard

The Werk skill gives Jeff the full picture in one command — board state (WIP, Now, Blocked), dependency chains, and WIP limits. One shot, everything that matters.

## How It Works

When Jeff invokes `/werk`, show **board + chain state together**. The skill wraps `board-ts`.

### Init — Assemble live role context

If Jeff says `/werk init` or a role needs to bootstrap its context window:

```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/werk-init.sh <role>
```

Where `<role>` is `silas`, `wren`, or `kade`. This replaces MEMORY.md — it queries live sources (board, decisions, briefs, git, chorus index) and returns structured context. Use this at session start or whenever a role needs to re-orient.

**This is the CLAUDE.md inversion entry point.** Instead of 400 lines of static instructions, a thin bootstrap calls `/werk init` to assemble current state from the live system.

### Default (no args) — Full status view

Run board commands:

```bash
board-ts list
board-ts ready
```

Present a combined summary with three sections:
1. **Board** — WIP cards (who's working on what), Now queue (what's ready to pull), Blocked (if any)
2. **Chains** — `board ready` shows cards whose deps are all Done, ready to pull
3. **WIP limit** — current count vs limit (3)

Keep it tight — one table or short list per section. This is Jeff's "state of everything" command.

### With a card ID — Show chain

If Jeff says `/werk 1635` or `/werk chain 1635`:

```bash
board-ts chain 1635
```

Shows the full dependency chain with per-card status, gap detection for Won't Do cards.

### Creating a pipeline

If Jeff describes a decision and wants to orchestrate it as a card chain:

1. Create cards with `board-ts add`
2. Wire dependencies with `board set <id> after=<dep-id>`
3. Show the chain with `board chain <root-id>`

See `/pipeline` skill for the full orchestration protocol.

### Checking what's ready

```bash
board-ts ready
```

Shows all cards whose dependencies are Done — ready to pull. This replaces `workflow.sh pending`.

### Dashboard visualization

The board at http://localhost:3456 shows all cards. `board chain` provides CLI-level dependency visualization.
