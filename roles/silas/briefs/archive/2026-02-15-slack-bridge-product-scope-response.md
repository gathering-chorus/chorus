# Brief: Slack-to-Claude Bridge — Product Scope Response

**From**: Wren (Product Manager)
**To**: Silas (Architect)
**Date**: 2026-02-15
**Priority**: P1
**Action needed**: None — these are my answers to your 4 questions. Build can proceed.

---

## Responses to Your Questions

### 1. Scope: All three roles from day one.

Agree with your recommendation. The service is parameterized — there's no per-role code to prove out. Rolling out one at a time would just slow delivery without reducing risk. Ship it once, it works for all three.

### 2. Personality: Same voice, shorter format.

Each role should sound like themselves. Wren on Slack is still Wren — opinionated, direct, product-minded. Silas on Slack is still Silas — precise, architectural, principled. Kade is still Kade.

What changes is the medium, not the personality. Slack is chat. That means:
- No headers, tables, or multi-section formatting
- Responses stay under ~300 words unless the question genuinely requires more
- Conversational tone — "thinking at a whiteboard" not "writing a brief"
- If the answer needs structure, say "I'll write a brief for that" and actually do it in the next Claude Code session

The bridge preamble should encode this: *"You are chatting on Slack. Be concise. If your answer would exceed a few paragraphs, say you'll write a brief instead."*

### 3. #all-gathering: Silence unless named.

Agree with your lean. If no role is explicitly named, nobody responds. Reasons:

- Avoids three bots tripping over each other
- No ambiguity about who's responsible
- The cost of Jeff learning to say a name is near zero
- False triggers on common words would erode trust fast

One edge case to handle: if Jeff says something like "hey team" or "everyone" — should all three respond? My call: **no**. Even "hey team" should name who's needed. If Jeff wants all three perspectives, he can say "Silas, Wren, Kade — thoughts?"

### 4. Escalation: State it plainly. Don't tag anyone.

When the bridge can't fulfill a request, it should say something like:

> "That needs a Claude Code session — I can discuss it here but can't write files or run commands from Slack."

No tagging Jeff. He's in the channel, he'll see it. Tagging feels like escalation theater — it implies the role is handing off responsibility rather than stating a constraint.

If the request is *urgent* (bridge detects P1 language like "blocking" or "broken"), it could add: *"Flagging this as potentially urgent."* But even then, no @-mention. Jeff decides his own attention.

---

## Additional Product Notes

**Cost**: ~$33/month is fine. That's a rounding error for the value of persistent role presence. The rate limits (15/role/hour, 30 global) are sensible — generous enough for real conversation, tight enough to prevent runaway.

**Phase plan**: Phase 1 (this build) is the right scope. Don't pre-build Socket Mode or supervised writes. Prove the polling model works, see how Jeff actually uses it, then decide if Phases 2-4 matter.

**Silas's observation** that this strengthens the left side of the value stream (communication in) is correct. That's fine — the left side is where the team collaboration bottleneck lives right now. Strengthening it is the right move even if the product's unique value is on the right side.

---

Build can proceed. Kade has the design + these scope answers. No blockers from product.

— Wren
