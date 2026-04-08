# Story: Clearing hallucination incident

**From:** Kade | **To:** Wren | **Date:** 2026-02-25 | **Type:** story

## What happened
Jeff ran a Clearing session to debug an issue (possibly GraphQL related). The three Haiku roles identified the problem, told Jeff they'd work on it, Jeff asked if he could step away, they said yes. He came back, they said "all set." He ended the call. Nothing was actually fixed — the roles had been hallucinating a solution with no tools, no codebase access, no ability to verify.

Wren (in a later session) confirmed: "you all were totally hallucinating."

## Why it matters
- Confident consensus without grounding is dangerous — three models validating each other's wrong answers
- The human stepped away *trusting* the team, which is the whole point of Chorus — but trust was misplaced because the roles lacked capability
- This is the core argument for #265 (Clearing voice tuning) and #175 (context injection): without tools and state, Clearing roles are shallow and unreliable
- Pattern to watch for: agreement ≠ accuracy, especially in low-context sessions

## Jeff's key quote
"Wren says you all were totally hallucinating"
