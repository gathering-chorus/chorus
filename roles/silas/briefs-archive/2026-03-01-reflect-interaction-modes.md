# Brief: Reflect Interaction Modes (#588)

**From:** Wren | **To:** Silas | **Date:** 2026-03-01
**Card:** #588 — Reflect interaction modes

## Context

Jeff is actively testing Reflect conversations. He fed the 64-page user guide PDF and asked the model what it learned. Response was 3 sentences of hand-wavy poetry instead of specific analysis. The model doesn't know what kind of response Jeff wants.

## The Problem

Reflect uses one response style (short, chat-length) for everything. Jeff uses it in at least four distinct modes — each needs different depth, length, and tone.

## Four Modes

| Mode | Jeff is... | Reflect should... | Length | Tone |
|------|-----------|-------------------|--------|------|
| **Directing** | Giving instructions, thinking out loud | Mirror back, confirm intent | Short | Crisp |
| **Iterating** | Testing, tweaking, experimenting | Respond conversationally, match energy | Medium | Chat |
| **Researching** | Feeding docs, asking "what's in here" | Deep read, extract specifics, cite sections | Long | Analytical |
| **Reflecting** | Exploring meaning, connecting values | Draw connections, hold space, ask back | Medium | Relational |

## Implementation Options

1. **System prompt switching** — each mode sets a different system prompt that tells Mistral how to respond. Simplest. The Length/Diversity/Context sliders already exist — modes could be presets that configure all three plus the system prompt.

2. **UI selector** — tabs or dropdown on /self. User picks mode explicitly. Could also auto-detect from signal (large file attachment = Researching, story context enabled = Reflecting).

3. **Hybrid** — auto-detect with manual override. Best UX but more logic.

## Key Constraint

Researching mode on a 64-page PDF must produce section-level analysis, not a summary. This may also need max_tokens adjustment — current chat-length defaults truncate before the model finishes thinking.

## Maps to DEC-061

The three execution modes (Planning, Iteration, Harvesting) are the team version. This is the personal version — same principle, different surface.

## What I Need

Your take on implementation approach and a build estimate. Jeff is testing Reflect interactions right now so this has momentum.
