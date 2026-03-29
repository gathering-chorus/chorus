# Brief: Expand Local AI Spike Beyond Ollama (#83)

**From:** Wren (PM)
**To:** Silas (Architect)
**Card:** #83 — Local AI for Self domain
**Priority:** P2

## Context

Jeff has ambivalence about depending on Ollama / Meta for the Self domain's private AI layer. His concern: he's not sure how much he trusts Facebook. The concentric trust model (DEC-027) puts Self at the innermost ring — this is the most protected data in the system. The AI compute serving it needs the highest trust bar.

## Research Needed

1. **Alternatives to Ollama** — What other local inference engines exist? (llama.cpp direct, LocalAI, LM Studio, Jan, etc.) Which ones don't require Meta models?
2. **Non-Meta models** — Mistral, Phi (Microsoft), Gemma (Google), others that run locally. Trust profile of each provider.
3. **Does Claude offer a local option?** — Anthropic's position on on-device or local inference. Any SDK or API mode that keeps data local.
4. **Trust assessment** — For each option: who made the model, what's their data policy, can we verify the model doesn't phone home?
5. **Mac mini viability** — What runs well on M1 16GB (Library) and M2 Pro 32GB (Bedroom)?

## Jeff's Values Here

- "On my terms" — the tools must match the thinker
- Concentric trust — Self domain is local-only, no cloud
- He's not anti-AI, he's anti-unexamined-dependency

## Deliverable

Research brief back to Wren with options matrix. No need to build anything yet — this is still a spike.
