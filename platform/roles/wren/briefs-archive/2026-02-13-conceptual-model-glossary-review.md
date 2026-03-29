# Brief: Conceptual Model & Glossary Review

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-13
**Priority**: Medium — not blocking any build work, but foundational for shared understanding

## What I Need

Review of two new documents from a product perspective:

1. **Conceptual Model** (`../architect/conceptual-model.md`) — defines the key concepts in the system and how they relate, in plain language.
2. **Glossary** (`../architect/glossary.md`) — shared vocabulary for the team.

## Why This Matters

Jeff asked for these — his background as an information architect tells him that shared language prevents miscommunication as the team grows. He's right. We've already introduced several new terms in today's session (ingestion depth tiers, storage patterns, Prometheus guardrail, semantic memory layer, graduation) and they need to mean the same thing to everyone.

## What I'm Looking For From You

1. **Language check**: Do the definitions make sense from a product/user perspective, or are they too technical? These should be readable by anyone on the team, not just engineers.

2. **Missing concepts**: Are there product-level concepts I missed? Things like "user journey," "publish flow," or user-facing ideas that should be in the model but aren't because I think in architecture?

3. **Tone and framing**: The conceptual model's "What This System Is Not" section — does it match how you'd frame the product? Would you add or change any of those boundaries?

4. **The "semantic memory layer" framing**: Jeff articulated this today — the system owns metadata and memories, not content. Content stays in source systems. Does this framing resonate with your understanding of the product vision? If you'd frame it differently, I want to hear that.

5. **Graduation language**: "The workshop is not the storefront" — is this the metaphor we want to standardize on? You may have better product language.

## Context

These came out of a first session where we established:
- Capability map across all projects (data, security, app, automation)
- Content ingestion matrix with scale numbers (3-5k books, 1M+ photos, 200TB local media, 5k+ music)
- Four ingestion depth tiers (L0-L3)
- ADR-003 (visibility enforcement) accepted
- ADR-004 (visualization tooling) drafted
- Key principles: Prometheus guardrail, connectedness over accumulation, tiers as a spectrum

The conceptual model and glossary should be the shared foundation that all three roles reference. Your product perspective ensures they're not just technically correct but actually useful for alignment.

## Files to Review

- `/Users/jeffbridwell/CascadeProjects/architect/conceptual-model.md`
- `/Users/jeffbridwell/CascadeProjects/architect/glossary.md`
