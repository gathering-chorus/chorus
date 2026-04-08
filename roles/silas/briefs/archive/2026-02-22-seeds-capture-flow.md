# Brief: Seeds/Capture Flow — Reconnect Intake Pipeline

**From:** Wren
**To:** Silas
**Date:** 2026-02-22
**Card:** #126

## Context

The Slack bridge was just cut (card #123, Kade finished today). That means the intake pipeline for new material into Gathering is disconnected. Jeff's capture flow — SMS → Notes → triage → Gathering — has no automated path right now.

Today Jeff texted himself 6 ideas during meditation and had to manually paste screenshots into our session. That's the friction this card fixes.

## What's Needed

Design the post-bridge capture architecture. Key questions:

1. **What replaces the bridge as intake?** The bridge watched Slack channels. With Slack deprecated, what watches what?
2. **SMS/Notes path**: Jeff texts himself → Apple Notes. How do we get from Notes to a card/seed? Card #95 (Notes harvester, Kade, now in Now) is the build side — you design the flow.
3. **Manual capture**: Jeff pastes things into sessions. Should there be a `/seed` skill that captures material with metadata?
4. **Routing**: Once captured, how does material get to the right place? (Gathering collection, Self domain, Chorus index, card on board)

## Constraints

- Must work on the two Mac minis (local, no cloud dependencies for capture)
- Jeff's typing is physical friction — minimize keyboard input
- Card #95 (Notes harvester) is in Now for Kade — coordinate with that

## Deliverable

Architecture brief with flow diagram: capture sources → intake mechanism → routing → destinations. Doesn't need to be built yet — just designed so Kade can build it.
