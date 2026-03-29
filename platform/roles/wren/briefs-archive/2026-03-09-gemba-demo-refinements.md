# Gemba + Demo Observer Refinements — #1208

**From:** Silas (Architect)
**Date:** 2026-03-09
**Re:** Changes to `/gemba` and `/demo` skills from today's live observation

## What changed

Jeff and I refined the gemba/demo observer flow based on friction we hit during your #1220 demo. Four changes shipped to the skill files:

1. **Demo observer collapsed into gemba** — `/demo` SKILL.md no longer duplicates the observer loop. It just says "nudge other roles into `/gemba <builder>`." One pattern, one place.

2. **Fast entry with card ID** — `/gemba <role> [card-id]` now accepts an optional card ID. Entry is 3 parallel calls max: `board-ts view`, tail snapshot, declare state. No file exploration, no artifact reading, no agents. I burned 2-3 minutes on context-building during your demo — that's fixed.

3. **Self-sustaining cron loop** — Observer sets up a `*/1 * * * *` cron that keeps the digest cycle alive. Jeff doesn't need to re-invoke `/gemba` each time. Observer holds attention, Jeff interjects freely, loop continues.

4. **10-minute TTL** — Demos auto-exit after 10 minutes with a debrief. Longer than that means stop and regroup.

## Also carded

- **#1224** — Auto-drain nudge inbox on session start and busy→idle transition. Nudges that don't land pile up in `/tmp/voice-inbox/<role>/` and nobody checks.

## What you should know

- The `/demo` skill still owns the builder flow (smoke check, prep brief, signal, show). Only the observer section changed.
- Your nudge to me during the demo hit exchange limit (4 max). The brief pathway worked as fallback, but #1224 will help with stale inbox buildup.
- Jeff also noted the demo scroll granularity is coarse — no card yet but it's a known friction point.

## Files changed

- `~/.claude/skills/gemba/SKILL.md` — rewritten entry + loop + TTL
- `~/.claude/skills/demo/SKILL.md` — observer section collapsed to point at gemba
