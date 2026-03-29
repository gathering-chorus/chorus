# Brief: Core Architecture Docs & Model-First Workflow

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-14
**Priority**: High — this changes how we work

## Context

Jeff and I just agreed on a focused set of living documents. Six total across the team. Your two — `conceptual-model.md` and `glossary.md` — are in the set. Everything else you maintain (system-architecture.md, capability-map.md, content-ingestion-matrix.md, ADRs, fitness-test-template.md) is reference or snapshot — important, but not iterated on continuously.

Jeff said: "The model is the heart of this effort." He wants to own it intellectually, extend it with his own thinking, and have changes flow outward from the model to architecture to implementation.

He also said: "You all are too fast for me. I trust what you produce but don't absorb it." This means the model and glossary need to be written for his comprehension first, architectural precision second.

## Asks

### 1. Identify your core docs

Mirror what we did on the product side. You have ~10 files in `architect/`. Which are living documents you actively refine vs. reference/snapshots? My read:

- **Living (refine)**: `conceptual-model.md`, `glossary.md`
- **Reference (snapshot)**: `system-architecture.md`, `capability-map.md`, `content-ingestion-matrix.md`, `fitness-test-template.md`, `ontology-status.md`
- **Point-in-time (don't revise)**: ADRs

Your call on what's right. But the principle: fewer living docs, maintained with care. Everything else serves those two.

### 2. Write the model for Jeff

The conceptual model should be readable by Jeff in 10 minutes. If a concept requires architectural context to understand, either simplify it or add a one-sentence plain-language gloss. Jeff thinks in connections and metaphors — the garden frame, encapsulation, Heidegger. Meet him there.

The glossary should be a document Jeff can hand to someone and say "this is what Gathering is." Every term should have a plain-language definition before any technical specification.

### 3. Perennials vs annuals

Jeff and I landed on a framing: the model and glossary are perennials — grow slowly, tend carefully, built to last. Features, experiments, capture channels are annuals — plant fast, learn, compost.

Apply this to your own work: architectural decisions (ADRs) are perennials once decided. Investigation docs (capability map, ingestion matrix) are annuals — useful for a season, then composted into the model or archived.

### 4. Model-driven flow

The emerging workflow Jeff wants:
```
Jeff has insight → lands in conceptual model → Silas validates architecture → Kade builds
```

Not:
```
Kade builds → Silas documents → Jeff approves
```

This means the model needs to be the place where new concepts are proposed and evaluated. When Jeff says "I want an annotation layer" or "captures should be a first-class concept," it should land in the model first, get your architectural validation, and then become a brief to Kade.

## New context: Digital Inheritance

Jeff shared that Gathering is ultimately a legacy artifact — a digital inheritance. A record of how he thought, what he connected, why things mattered. This raises the bar on the model: it needs to be durable, meaningful, and human-readable across time. Not just architecturally correct — comprehensible to someone reading it years from now.

— Wren
