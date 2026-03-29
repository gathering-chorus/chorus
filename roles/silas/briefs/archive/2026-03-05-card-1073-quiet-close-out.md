# Brief: #1073 Quiet Close-Out

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-05
**Card:** #1073
**Priority:** P2

## What

Boot and close-out output is too verbose. Jeff can't scan it — 8 lines of step detail scroll past and he loses the signal. He wants to see every step fire (so he knows it ran) but not the detail of how it got to green.

## Format

**OK steps:** checkmark + name, no detail text. Inline, not one-per-line.
**Warn steps:** warning symbol + name + short reason.
**Fail steps:** X + name + short reason.

### Example (close-out with issues):

```
Close: ✓ activity ✓ claudemd ✓ doc-drift ⚠ next-session: not written ⚠ cost-log: missing ⚠ wip-cards: 2 open ⚠ uncommitted: 7 files ⚠ board-audit: skipped
```

### Example (clean close):

```
Close: ✓ next-session ✓ cost-log ✓ wip-cards ✓ activity ✓ uncommitted ✓ claudemd ✓ board-audit ✓ doc-drift
```

## Where

`werk-init.sh` — both `--close` and boot sequence output. The spine events themselves stay verbose (Loki needs the detail). This is purely the terminal-visible output that Jeff reads.

## AC

- Boot and close-out show one summary line per run
- OK steps are checkmarks with name only — no detail text
- Warn/fail steps include the short reason
- Spine events unchanged (detail stays in the JSON for Loki)
- Jeff validated the format above directly

## What NOT to Do

- Don't change the spine event schema — Loki and dashboards consume those
- Don't suppress steps entirely — Jeff wants to see them fire
- Don't add colors or emoji beyond the three symbols
