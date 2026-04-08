# Brief: Reflect Test Results — Wren

**From:** Wren | **To:** Silas | **Card:** #532 | **Date:** 2026-02-28

## Test Results

Confirmed your findings. All three tiers pass word count. Three issues to fix, in priority order:

### 1. Mid-sentence truncation (worst UX issue)
Every medium and deep response cuts off mid-word ("pratitya samutp—", "is inter—"). Feels broken to the user.

**Fix options:**
- Set max_tokens ~20% below the cap, then add a system prompt instruction: "Complete your current thought within the token budget. Never end mid-sentence."
- Or: post-process — if the response doesn't end with sentence-ending punctuation, trim back to the last complete sentence.

### 2. Fence painting dominates all responses
All three of my tests referenced the fence painting — including "Hi Reflect" which has no topical connection. Random story selection is over-indexing on one story.

**Fix:** For greetings (≤5 words), skip RAG entirely. Just respond conversationally — no story context needed. For medium prompts, use semantic similarity (even basic keyword overlap) instead of random selection.

### 3. Greeting tone
"Hello Jeff, Your words about the art of gathering and finding balance resonate with the story of your fence painting" is not a greeting response.

**Fix:** Same as #2 — skip RAG for short inputs. The model's natural conversational response without story context will be better than forcing reflection on "Hi."

## What's Working
- Deep tier tone is genuinely good: "In the stillness of your outdoor meditation, where the rustling of leaves and the warmth of the sun converge" — that's the right voice.
- Response times are solid (1.9-3.8s).
- Token clamp is working correctly.
- The architecture is right — just needs tuning.

## Recommendation
Fix truncation first (post-process trim to last complete sentence). Then skip RAG for ≤5 word inputs. Those two changes make it demo-ready for Jeff.
