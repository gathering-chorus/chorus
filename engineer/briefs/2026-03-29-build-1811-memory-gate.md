# Build #1811 — Memory-and-research gate hook

**From:** Wren
**Date:** 2026-03-29
**Priority:** Next up after nudge testing

## Card
#1811 — reassigned to you. AC and Jeff's refinement are on the card (`board-ts view 1811`).

## Architecture (already researched)

Follow the existing hook pattern:
- `src/hooks/memory_gate.rs` — new file
- Same structure as `csc_guard.rs`: takes `HookInput`, returns `HookResponse`
- Register in `main.rs` alongside other PreToolUse hooks

## Session JSONL scanning
Reuse the pattern from `autonomy_guard.rs:378` (`read_last_messages_from_jsonl`). Scan last ~200 lines of the session JSONL for:
- **Memory check**: Read/Grep of `MEMORY.md`, `decisions/`, `briefs/`, or memory files
- **Research check**: `git log`, `git show`, `git blame`, or Read of the file being edited

## Jeff's refinement (from card comment)
- **Defect fixes** (card title contains fix/bug/broken/wrong/fails): ALWAYS enforce both checks
- **Enhancements, own domain, 0-2 files**: trust builder, don't enforce
- **Cross-domain**: enforce
- Sub-millisecond — same scan pattern as JDI hook

## Files to read first
1. `board-ts view 1811` — AC + Jeff's comment
2. `src/hooks/csc_guard.rs` — hook structure template
3. `src/hooks/autonomy_guard.rs:378` — JSONL scan pattern
4. `src/types.rs` — HookInput, HookResponse, permission_deny_json
5. `/tmp/pair-1764.md` — scratch file with additional context

## Test approach
Unit tests in the same file (see `nudge.rs` tests at bottom for pattern). Test cases from AC:
- Block when no prior memory/research check
- Pass when memory checked
- Pass when both checked
- Pass for small enhancement in own domain
