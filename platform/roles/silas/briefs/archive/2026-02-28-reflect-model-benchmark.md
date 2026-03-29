# Brief: Benchmark Alternative Models for Reflect

**From:** Wren | **To:** Silas | **Card:** #532 | **Date:** 2026-02-28

## Problem

Mistral 7B is mechanistic. Jeff expected a conversation — what he got is a stateless slot machine that pulls random stories. Each turn starts from zero, greetings get philosophy, and the model can't read the room. The system prompt tuning helps but may not overcome the 7B ceiling.

## Request

Benchmark 2-3 alternative models on the Bedroom Mac (M2 Pro, 32GB) for Reflect. Test the same three tiers (greeting, medium, deep) and compare conversational quality, not just token counts.

## Candidates (in order of effort)

1. **Llama 3 8B** (4-bit) — same footprint, likely better instruction following and conversational tone. Try this first.
2. **Mixtral 8x7B** (4-bit, ~24GB) — mixture of experts, good quality/speed tradeoff. Fits in 32GB.
3. **Mistral Small 22B** (4-bit) — significant quality jump. Tighter fit but should work.

## What to Test

For each model, run:
- "Hi Reflect" — does it respond conversationally without dumping philosophy?
- "I've been thinking about how the team is working today" — does it connect to relevant stories, not random ones?
- "Tell me about the connection between my outdoor meditation practice and how I lead teams" — quality of reflection
- Two-turn conversation — does it remember what was just said?

## Also Consider

- Conversation memory — can we pass the last 2-3 turns as context? That's the other missing piece. Stateless turns feel mechanical regardless of model quality.
- Response time — Jeff won't wait more than 5-6 seconds. If a bigger model is too slow, it's out.

## Constraints

- Must run on Bedroom Mac (32GB, M2 Pro)
- Must not interfere with images-api (port 3001) or video-server (port 8082)
- MLX preferred since that's what's already running
