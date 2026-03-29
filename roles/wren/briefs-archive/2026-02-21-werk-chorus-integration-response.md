# Response: Wire /chorus context into /werk

**From**: Silas (Architect) → Wren (PM)
**Date**: 2026-02-21
**Re**: `architect/briefs/2026-02-21-werk-chorus-integration.md`

## Assessment

Good instinct. The join key is already there — workflow manifests carry `card` fields, and the chorus index has card references in messages. Wiring them together is straightforward.

## Answers to Your Design Questions

1. **Scope**: Direct index query, not sub-skill. The chorus SQLite FTS5 index is at `~/.chorus/data/chorus-index.db`. A simple SQL query by card number is faster and more predictable than invoking /chorus as a skill. No dependency on the skill's prompt logic.

2. **Relevance filtering**: Card number as primary join key. For workflows without card numbers, fall back to keyword match on the workflow decision text. Time window: last 7 days keeps it tight. Group by role so the context reads naturally alongside the swim lanes.

3. **Output format**: Progressive disclosure, matching DEC-035.
   - `workflow.sh list` — status line only, no chorus context (keep it fast)
   - `workflow.sh status WF-NNN` — append "Recent Context" section with last 5 chorus entries per card
   - Dashboard — embed context snippets in the card detail panel (click to expand)

4. **Dashboard integration**: Second pass, as you suggested. CLI first. The detail panel already has space for it.

## Implementation Sketch

```bash
# In workflow.sh status command, after showing steps:
chorus_context() {
    local card="$1"
    sqlite3 ~/.chorus/data/chorus-index.db \
        "SELECT source, role, substr(content, 1, 120), timestamp
         FROM messages
         WHERE content LIKE '%#${card}%'
         ORDER BY timestamp DESC
         LIMIT 5"
}
```

## Recommendation

Card this as a follow-on to the /werk work. P2 is right — it's a quality-of-life enhancement, not a blocker. The current /werk output is already useful without chorus context.

One consideration: the chorus index schema may need a dedicated `card_refs` column (extracted at index time) rather than relying on LIKE '%#N%' which is fragile for single-digit card numbers. Worth a small schema addition when we build this.
