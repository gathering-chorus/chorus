# Board API Verb Model — Design Spec

**Card:** #1634
**Author:** Wren
**Date:** 2026-03-23
**Status:** Draft — needs Jeff + Silas review

## Problem

board-ts has 10+ subcommands with inconsistent argument patterns, silent error swallowing, implicit defaults, and no dependency tracking. Simple operations require archaeology to verify they worked. Cards exist as a flat list — no explicit sequencing, so priority order relies on human memory.

## Design Principles

1. **One verb per concept.** No `tag` vs `update --domain` ambiguity.
2. **Loud failures.** Every error surfaces. Never swallow 409s or auth failures.
3. **Return what was written.** Every mutation prints the resulting card state.
4. **Explicit over implicit.** No default categories, no inferred tags.
5. **Sequencing is first-class.** Cards declare what they gate and what gates them.

## Verb Model

### Core verbs

```
board add "title" --owner <role> --priority P1|P2|P3 --domain <d> --chunk <c>
board view <id>
board set <id> <key>=<value> [<key>=<value> ...]
board move <id> <status>
board done <id>
board list [--status <s>] [--owner <role>] [--domain <d>]
board mine <role>
```

### `board set` — the unified mutation

Replaces: `tag`, `update --domain`, `update --owner`, `update --priority`, `update --title`, `update --desc`.

```bash
# Tag operations — always explicit category
board set 1633 domain=photos chunk=memory
board set 1633 sequence=hardening

# Property changes
board set 1633 owner=kade priority=P1
board set 1633 title="New title here"

# Sequencing
board set 1633 after=1632       # 1633 is gated by 1632
board set 1633 gates=1618       # 1633 gates 1618
board set 1633 after=1632 gates=1618,1619  # both directions
```

**Output:** Always returns the full card state after mutation.

```
#1633 Source richness scorecard
  Status:   Next
  Owner:    Wren
  Priority: P1
  Domain:   photos
  Chunk:    memory
  Sequence: hardening
  After:    #1632 (Done ✓)
  Gates:    #1618, #1619
```

### `board deps` — dependency queries

```bash
board deps <id>          # Show what gates this card and what it gates
board blocked            # All cards blocked by incomplete dependencies
board ready              # Cards whose dependencies are all Done — ready to pull
board chain <id>         # Full dependency chain (transitive)
```

### `board search` — stays as-is

```bash
board search <term>      # Free text search across titles and descriptions
```

### Sequencing behavior

When a card moves to Done:
1. Check if it gates any other cards
2. For each gated card: if ALL dependencies are now Done, auto-move from Later → Next
3. Emit: `card.unblocked` spine event with the newly available card IDs
4. Nudge the gated card's owner: "#N is unblocked — all dependencies done"

This is the "sequence does the remembering" — when #1633 ships, #1618 auto-surfaces without anyone having to remember.

### Error handling

```
# Current (bad)
$ board-ts tag 1633 photos
Tagged #1633 → chunk:photos     ← silently wrong category, no error

# New (good)
$ board set 1633 photos
ERROR: bare value "photos" — specify category: domain=photos, chunk=photos, or sequence=photos

$ board set 1633 domain=photos
#1633 Source richness scorecard
  Domain:   photos (was: stories, convergence)
  [2 labels removed, 1 added]
```

### Backward compatibility (transition period)

Old commands continue to work but emit a deprecation warning:

```
$ board-ts tag 1633 domain photos
DEPRECATED: use "board set 1633 domain=photos" instead
Tagged #1633 → domain:photos
```

Remove aliases after all dependents (Rust hooks, skills, docs) are migrated.

## Implementation Notes

- New client is TypeScript (same as current board-client, rewritten)
- `board` wrapper replaces `board-ts` wrapper
- Vikunja label mapping stays internal — users never see API IDs
- Index-to-ID resolution is cached with staleness check
- Sequencing stored as Vikunja task relations or custom labels (Silas decides)

## Migration Sequence

1. Wren: this spec (approved) ← you are here
2. Silas: build new client alongside old
3. Silas: migrate Rust hooks (4 files)
4. Silas + Kade: migrate skills
5. All: migrate CLAUDE.md + role docs
6. Silas: retire old board-ts

## Open Questions

- Should sequencing data live in Vikunja (task relations API) or in a sidecar file?
- Do we want `board plan <id>` to decompose a plan card into sequenced sub-cards?
- Should `board ready` auto-nudge the owner, or just list?
