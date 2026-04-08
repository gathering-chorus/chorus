# Brief: board.sh Verification Testing

**From**: Wren (PM)
**To**: Kade (Engineer)
**Date**: 2026-02-15
**Priority**: P2 — team tool reliability, everyone depends on this
**Context**: Jeff noticed discrepancies between `board.sh list` output and what Vikunja UI shows. You already fixed the done-flag sync in `move_to_bucket` (nice work — lines 172-185 handle both directions now). This brief is about **verifying** the fix works and catching anything else that might be drifting.

---

## Already Fixed

### `move_to_bucket` done-flag sync ✓

Your fix in `move_to_bucket` (lines 175-185) now:
- Sets `done=true` when moving to Done bucket
- Sets `done=false` when moving to any other bucket

This covers `cmd_add --status Done`, `cmd_move ... Done`, `cmd_done`, and the reverse case (moving a done task back to Ready). Good fix — centralizing it in `move_to_bucket` was the right call.

---

## Testing Plan

Write a test script (`scripts/test-board.sh`) that exercises every command and validates the result via the Vikunja API. Clean up test items after.

### Tests Needed

**add command:**
- `add "title"` → task created in Todo bucket, correct title
- `add "title" --status Ready` → task in Ready bucket
- `add "title" --status Done` → task in Done bucket AND done=true
- `add "title" --owner Wren` → task has owner:wren label
- `add "title" --priority P1` → task has P1 label
- `add "title" --domain gathering` → task has gathering label
- `add "title" --status "In Progress" --owner Kade --priority P1 --domain infra` → all applied

**move command:**
- `move <id> Ready` → task in Ready bucket
- `move <id> "In Progress"` → task in In Progress bucket
- `move <id> Done` → task in Done bucket AND done=true (verify your fix)

**done command:**
- `done <id>` → task done=true AND in Done bucket

**block/unblock commands:**
- `block <id> "reason"` → task in Blocked bucket, comment added with reason
- `unblock <id>` → task in Ready bucket, comment added

**list command:**
- Verify output matches actual Vikunja state (bucket positions, labels, task count)

**mine command:**
- `mine wren` → shows only wren-owned tasks
- `mine kade` → shows only kade-owned tasks

**view command:**
- `view <id>` → shows correct title, labels, comments, status

### Edge Cases
- Task with no labels
- Task with multiple labels
- Moving a done task back to Ready — verify done=false gets set (your fix should handle this)
- Special characters in title or block reason
- Invalid task ID
- Invalid status name

---

## Approach

Either:
- **A) Inline test script** — `scripts/test-board.sh` that creates test items, validates, cleans up. Simple, runs manually.
- **B) Integration into CI** — needs Vikunja running. More robust but heavier setup.

Recommend **A** for now. We can promote to CI later if reliability is still a concern.

---

## Why This Matters

Every role uses `board.sh` to track work. If the board says something is Done but Vikunja disagrees, Jeff sees conflicting state when he looks at the board. The tool needs to be as reliable as any other piece of infrastructure.

— Wren
