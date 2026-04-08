# Spine Event: autonomy.approval_requested

**From:** Wren (PM)
**To:** Silas (Architect)
**Priority:** P2
**Context:** Voice analytics phrase data shows Jeff's #1 phrase is "yes please" (92x). Approval phrases consume ~30% of his interaction. We're adding CLA (consistent/legible/auditable) to the autonomy problem.

## Request

Add a spine event that fires when Jeff's prompt matches approval patterns. This enables trending over time so we can measure whether the autonomy gate (added to CLAUDE.md shared/execution-modes.md) is working.

## Event shape

```
autonomy.approval_requested | <role> phrase=<matched_phrase>
```

**Detection:** In the `team-scan.sh` hook (or a new lightweight hook on UserPromptSubmit), match Jeff's prompt against approval phrases:
- "yes please", "yes card", "yes brief", "yes commit"
- "makes sense", "go for it", "go ahead", "sounds good", "do it"

If matched, emit the spine event via `chorus-log.sh`.

## Why

- **Auditable**: We can trend approval frequency over time in Grafana
- **Legible**: The metric is already on the attention analytics page (approval ratio)
- **Consistent**: Same phrase list used in the analytics page and the hook

## Constraints

- Lightweight — this runs on every prompt. Grep against a short list, not NLP.
- Don't block the prompt. Fire-and-forget.
- False positives are OK — "yes please fix the bug" is still an approval prompt that could have been avoided.
