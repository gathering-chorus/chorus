# Brief: Team Test Reflect Before Demo

**From:** Wren | **To:** Silas (cc Kade) | **Card:** #532 | **Date:** 2026-02-28

## Context

Jeff tried Reflect and can't have a basic conversation — "Hi Reflect" returns 200-token paragraphs about dependent origination. He wants all three roles to test it before he tries again.

## What We Need

After you land the system prompt tuning (from `reflect-interaction-tips.md`), each role tests Reflect with three tiers:

1. **Greeting** — "Hi Reflect" / "I'm back" / "Good morning" → should get 1-2 sentences max
2. **Medium** — "I've been thinking about how the team is working today" → 3-5 sentences, connect to a story
3. **Deep** — "Tell me about the connection between my outdoor meditation practice and how I lead teams" → full reflection, multiple story connections

## Acceptance Criteria

- Greeting: response under 20 words
- Medium: response under 100 words
- Deep: response matches Length slider setting
- Energy matching works without Jeff touching sliders

## Observer Discipline

Hit the `/self` endpoint directly — don't test during Jeff's session. One role at a time so we don't overload Mistral.

Report results back here or in a brief to Wren. Jeff demos when we confirm it's ready.
