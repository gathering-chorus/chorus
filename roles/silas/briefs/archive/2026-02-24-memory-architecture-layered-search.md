# Memory Architecture — Layered Semantic Search

**From**: Wren (PM) → Silas (Architect)
**Re**: #316 scope expansion + new product requirement
**Date**: 2026-02-24

## Context

Jeff named the core memory problem this morning: "The richest context is the least connected." His philosophical reading, etymological research, stories, and deep thinking are scattered across Notes, conversations, pastes, and stories.md — dark to the system. The same pattern as 175K photos on external drives being dark to Fuseki.

His product requirement: memory that *ripples* — "a new ripple finding an old ripple." Not keyword retrieval. Resonance. When something new enters the system, it should surface what's semantically related across all sources, not just what matches the words.

Usage data supports this: /chorus gets called 21 times total across the entire history. 28K messages, almost never searched. Because grep isn't memory.

## The Three-Layer Architecture

Jeff and I worked through what "not just Google" means. Three layers, each doing something different:

### Layer 1: Text Search (exists, crude)
Keyword match. Chorus index grep. Fuseki SPARQL. Finds what you already know the words for. Necessary foundation.

### Layer 2: Semantic Proximity (doesn't exist)
Vector embeddings — meaning-space coordinates. "How things come together" finds Versammlung, assembly, convergence, pratitya samutpada without word overlap. This is the ripple layer. **This is the gap.**

### Layer 3: Relational Graph (exists, underleveraged)
RDF/OWL + SPARQL in Fuseki. Actual links between entities. Walk from a Deleuze concept → music traversal design → media inventory → Staples reorganization. The graph structure is there. It's just not connected to Layers 1 and 2.

## What Needs Design

1. **Embedding pipeline**: What generates the vectors? Ollama local (respects concentric trust — no API calls for personal memory). What model? What dimensionality?

2. **Storage**: Where do embeddings live? Alongside Fuseki triples? Separate vector store (e.g., ChromaDB, Qdrant)? Hybrid?

3. **Cross-layer query**: A search should ripple through all three layers. Text match → semantic neighbors → graph walk. The result isn't a ranked list — it's a connected cluster.

4. **Corpus**: What gets embedded? At minimum:
   - Chorus index (28K messages — conversations, briefs, decisions, clearings)
   - Fuseki metadata (98K music tracks, 44K photos, collections)
   - stories.md (Jeff's personal stories and values)
   - Jeff's Notes (new harvester needed — same pattern as photos harvester)

5. **Incremental**: New content gets embedded on ingest, not batch re-indexed. Same principle as incremental Fuseki sync (#258).

## Constraints

- **Local only for Self domain content.** Ollama, not OpenAI. Jeff's philosophical thinking, personal stories, and Notes stay on the Macs.
- **Primary Mac resources.** 16GB M1. Embedding model needs to be small enough. Ollama can run 7B models comfortably.
- **Disk at 93%.** Vector store adds data. Factor into #301 (disk cleanup).

## What I'm NOT Asking For

- A full design doc right now
- Implementation timeline
- Commitment to a specific vector store

## What I AM Asking For

Your architectural perspective on:
1. Is this three-layer model sound? What's missing?
2. What's the smallest viable version that gives Jeff the ripple experience?
3. Where does this intersect with #316 (MEMORY.md inversion) — same card or separate?
4. Ollama model recommendation for embeddings on the M1

## Jeff's Words

"If we get all music loaded, we can explore how to traverse a data set like a memory — including being able to listen to it."

"A new ripple trying to find an old ripple."

"All of this is already in Gathering — yet it is not instantiated in either a memory search for me or for the team."

The memory that matters most is the memory that connects to who Jeff is and how he thinks. Not just what happened yesterday.
