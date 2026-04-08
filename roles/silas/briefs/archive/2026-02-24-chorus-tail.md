# Brief: Chorus Tail — Live Shoulder Tap

**From**: Wren | **To**: Silas | **Card**: #337 | **Priority**: P1
**Context**: DEC-047 (dense spine events). Today we watched Kade build Stories (#330) and couldn't see anything happening until after the fact. Jeff relayed status to Wren 3 times — "he's compiling," "adding tests," "deployed." The spine was dark.

## What

`chorus tail <role>` — streams the last N entries from a role's active JSONL session transcript. Like `tail -f` for the spine. No interrupt, no message — just visibility.

## Use Cases

1. **Jeff watches Kade build** — sees tool calls, file creates, compile results without opening Kade's terminal
2. **Silas tails Kade during SWAT** — sees build failures in real time, correlates with Loki, diagnoses without Jeff typing
3. **Wren tails any role** — sees whether brief was read, whether plan mode was entered, whether scope crept
4. **Roles tail each other** — self-tuning team, no relay needed

## Implementation

The JSONL session files are at `~/.claude/projects/*/` (one per session). The active session is the most recent file being written to.

### CLI: `chorus-query.sh tail <role>`

1. Find the most recent JSONL for the role (by modified time)
2. Parse last N entries (default 10, configurable with `--lines`)
3. Format: timestamp, role (user/assistant), truncated content (first 120 chars)
4. Optional `--watch` flag: re-read every 5s and show new entries (like `tail -f`)

### Output Format

```
12:15:03 [assistant] Reading brief: stories-collection.md...
12:15:08 [assistant] Tool: Write → src/interfaces/stories.interface.ts
12:15:12 [assistant] Tool: Write → src/services/stories-pod.service.ts
12:15:15 [assistant] Tool: Bash → npx tsc --noEmit (exit 0)
12:15:20 [assistant] Tool: Bash → npm test (2245 pass, 0 fail)
12:15:25 [assistant] Tool: Bash → app-state.sh deploy
```

### Integration with /chorus skill

Add `tail` as a subcommand alongside `search`, `reconcile`, `role`, `stats`. Usage: `/chorus tail kade` or `/chorus tail silas --lines 20`.

## Acceptance Criteria

- [ ] `chorus-query.sh tail kade` shows last 10 entries from Kade's active session
- [ ] `--lines N` controls count
- [ ] Output is concise — one line per entry, tool calls show tool name + first arg
- [ ] Works from any role's session (Wren can tail Kade, Silas can tail Wren)
- [ ] Doesn't interfere with the active session (read-only on the JSONL)

## Not In Scope

- `--watch` mode (Phase 2)
- Filtering by tool type or content
- Writing tail output to chorus index
- Dense spine events (#338) — that's the next layer after tail exists
