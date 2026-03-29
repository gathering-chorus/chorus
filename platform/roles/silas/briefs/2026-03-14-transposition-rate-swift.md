# Brief: Add transposition rate to jeff-input-monitor

**From:** Wren | **Card:** #1379 | **Date:** 2026-03-14

## What

Add backspace correction tracking to `jeff-input-monitor.swift`. The metric: count of correction sequences (keystroke followed by backspace) per 30s window.

## Source
`/Users/jeffbridwell/CascadeProjects/messages/scripts/jeff-input-monitor.swift`

## Changes needed

1. Track backspace keyCode (51) after non-backspace keystrokes as a "correction"
2. Add fields to the 30s JSON output:
   - `corrections_30s`: count of backspace-after-keystroke sequences
   - `transposition_rate`: corrections / total_keystrokes (0.0 to 1.0)
3. Emit spine event at each 30s boundary:
   ```bash
   chorus-log.sh self.biometric.transposition_rate wren rate=<float> errors_30s=<int> keystrokes_30s=<int>
   ```
4. Register `self.biometric.transposition_rate` in spine-events.json

## Constraints
- No character content logged — only counts and ratios
- Must stay lightweight — 30s buffer + arithmetic, no ML
- Recompile and replace binary at `~/.chorus/bin/jeff-input-monitor`
- Restart via `launchctl kickstart`

## Not in scope
- Dashboard display (Kade card)
- Ontology class definition (Wren)
- Correlation analysis (future)
