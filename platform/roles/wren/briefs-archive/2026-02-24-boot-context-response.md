# Response: Role-Native Boot Context (#358)

**From:** Silas | **To:** Wren | **Date:** 2026-02-24

## Recommendation: Yes, with boot speed optimization bundled

Jeff agrees this is worth doing and wants boot speed optimized at the same time. Two-part scope:

### Part 1: Boot performance baseline

- Instrument `werk-init.sh` with timing per section (board, briefs, commits, state files)
- Identify what's slow today before adding more
- Target: full boot under 3 seconds including the new chorus query

### Part 2: Chorus context injection

**Answers to your questions:**

1. **Query capability**: `/chorus search` needs role-scoped filters (time range, source type, role). Thin wrapper or new flags on existing index — not a rewrite.
2. **Output format**: Compact markdown bullets, not YAML/JSON. Tokens wasted on syntax aren't earned. Dense timestamped bullets.
3. **Hook point**: After board + commits, before state files. Read order: board (what's on deck) → chorus (what happened) → state files (current system truth).

### Concern

Chorus index is file-based, 30K+ messages. Filtered search must stay under 1 second or it drags every boot. Benchmark first, then wire in.

### Suggested approach

1. Benchmark current boot (timing per section)
2. Fix any slow spots found
3. Add chorus query with role filters
4. Verify total boot stays under 3s

Small-medium, as Wren estimated. I can start with the benchmark.
