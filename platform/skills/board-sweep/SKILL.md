---
name: board-sweep
description: Board coherence sweep — find and fix untagged, duplicate, and stale cards. Scheduled or on-demand.
user-invocable: true
---

# /board-sweep — Board Coherence Check

Scan the board for hygiene issues. Fix what you can, report what you find. No reimagining of sequences or chunks — only tag what's missing, close what's stale, and flag what needs Jeff.

## When to run

- **Scheduled**: every 4 hours via cron (Wren sets this up at session start)
- **On-demand**: Jeff says `/board-sweep` or "clean the board"
- **After a big build session**: when 5+ cards ship in one session, cruft follows

## How to Execute

### Step 1: Pull the full board

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/board-ts list 2>&1
```

### Step 2: Check for issues

Scan for these five problems, in order:

**1. Untagged cards** — active cards (Now, WIP, Next, Later) missing `chunk:` label.
- **Fix**: infer chunk from title, domain labels, or owner. Tag it.
- **Don't**: reimagine existing chunks or create new ones. Use the valid set: spine, ops, memory, music, senses, strategy, app, sexuality.

**2. Duplicate cards** — two cards with the same intent in active columns.
- **Fix**: close the newer one with a comment linking to the original. `board-ts comment <id> "Duplicate of #<original>. Closed in board sweep."`
- **Don't**: merge descriptions or try to pick the "better" one. Close the dupe, keep the original.

**3. Stale blocked cards** — cards in Blocked for >7 days with no recent comment.
- **Fix**: comment asking if the blocker is still real. `board-ts comment <id> "Blocked >7 days — is the blocker still active?"`
- **Don't**: unblock or move them. Flag for Jeff.

**4. Ownerless Now cards** — cards in Now with no `owner:` label.
- **Fix**: infer owner from chunk/domain. Tag it.
- **Don't**: pull cards to WIP or assign work.

**5. Stale auto-error cards** — `[auto-error]` or `[defect]` cards in active columns that are >7 days old with no recent activity.
- **Fix**: move to Won't Do with comment. These are noise if nobody's working them.

**6. Memory drift** — check MEMORY.md for stale facts.
- **Check**: scan MEMORY.md for infrastructure references (disk %, test count, deploy times, retired tooling). Compare against known current state.
- **Fix**: update stale entries directly. Log what changed.
- **Don't**: rewrite memory files. Only fix facts that are provably wrong.

### Step 3: Report

**If issues found**: print a summary table:

```
Board sweep: <N> issues found
- <N> untagged → tagged
- <N> duplicates → closed
- <N> stale blocked → flagged
- <N> auto-errors → closed
```

Then `/ot flow` to show the result.

**If no issues**: one line: `Board sweep: clean.` No `/ot`.

### Step 4: Emit spine event

```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log board.sweep.completed wren issues=<count>
```

## Rules

- **Don't reimagine existing tags.** If a card has a chunk, leave it. Only tag what's missing.
- **Don't move cards between columns** except auto-errors to Won't Do. Board flow is Jeff's or the owning role's decision.
- **Don't rewrite card descriptions or AC.** Sweep is hygiene, not editing.
- **Don't create new cards** from sweep findings. If something needs a card, tell Jeff.
- **Silent when clean.** Jeff doesn't need to know the board is fine. He needs to know when it's not.
- **DEC-091 applies** — this is housekeeping. JDI.
