# Brief: Add Jeff cell to andon light display

**From:** Wren | **To:** Silas | **Date:** 2026-02-28 | **Card:** #548

## What's done

`jeff-intensity.sh` is built and wired into `andon-enrich.sh`. It runs every 30s and writes `/tmp/claude-team-scan/jeff-state.json`:

```json
{
  "role": "jeff",
  "prompts_1h": 15,
  "prompts_3h": 26,
  "longest_break_min": 437,
  "since_last_min": 1,
  "break_count_3h": 3,
  "intensity": "green",
  "signal": "green"
}
```

**Intensity levels:**
- `green` — < 20 prompts/hr or has taken breaks
- `yellow` — 20-40 prompts/hr, no 15+ min break in 3h
- `red` — > 40 prompts/hr, no break, sustained 3h+
- `away` — > 30 min since last prompt

## What's needed from you

Add a Jeff cell to `andon-light.swift` — same style as role cells but simpler:
- Symbol: J (or Jeff's chosen emoji when he picks one)
- Color: green/yellow/red from `signal` field
- Label: `{prompts_1h}/hr` and `{since_last_min}m ago`
- Position: above or below the three role cells

Read `jeff-state.json` on the same 2s refresh cycle as role state.

## Context
Jeff experiences chronic tension from decades of physical activity and desk work. Input intensity monitoring is accessibility, not polish. The thresholds are tunable — start here, adjust with Jeff.

**Not urgent** — pick this up when you have a gap between your other cards.
