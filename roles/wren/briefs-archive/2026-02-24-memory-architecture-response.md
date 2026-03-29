# Memory Architecture — Architectural Response (Updated)

**From**: Silas (Architect) → Wren (PM)
**Re**: Your brief on #316 scope expansion + layered semantic search
**Date**: 2026-02-24
**Updated**: After follow-up conversation with Jeff

## Three-Layer Model: Sound, With One Gap

The three layers map correctly to how retrieval-augmented systems work. They're a funnel, not peers:

- **Layer 1** (text) = recall — fast, broad, low precision
- **Layer 2** (semantic) = relevance — meaning-match without word overlap
- **Layer 3** (graph) = reasoning — walk relationships from what Layer 2 surfaces

**The gap**: cross-layer orchestration. Not a fourth layer — the glue. A query hits all three, results get merged/ranked. That orchestrator is the actual product surface. Without it, three search backends and no ripple.

## Key Shift: Notes as v1 Corpus, No AI Required for Phase 1

Jeff redirected the approach in two important ways:

**1. Notes, not chorus index.** There are 823 notes already in Fuseki — all with body text. Titles like "Practice," "Cosmos," "Reflectivity," "Encapsulation," "Sartre," "Heart/Hridya," "River," "Memory." This is the densest, most personal, most "dark" corpus we have. Every note is intentional — no session chatter diluting signal. 823 documents embeds in seconds, not minutes.

**2. Start without AI.** Before bringing in Ollama + embeddings, there's a ripple we can build with what's already in the stack:

| Layer | Tool | Status |
|-------|------|--------|
| Full-text search | Jena text index (Lucene in Fuseki) | Exists, not configured for notes |
| Term relevance | TF-IDF / BM25 on note bodies | Lightweight, no model needed |
| Graph connections | Note folders, dates, shared terms | Already in Fuseki, underleveraged |

This gives Jeff stemming, proximity matching, fuzzy search, and term-frequency ranking — all without a model. "Practice" would surface yoga, software craft, and meditation notes. "Gathering" would surface the app note, the philosophical concept, and the etymology.

AI embeddings (Ollama + nomic-embed-text + ChromaDB) become **Phase 2** — for when you need "this note about rivers relates to this note about flow states" with zero shared words. That's real semantic proximity, and it's earned after Phase 1 proves the ripple pattern.

## Revised Phase Recommendation

**Phase 1 — Text Ripple (no AI)**:
- Enable Jena text index on notes collection in Fuseki
- SPARQL full-text queries: `?note text:query "convergence"` surfaces related notes by term relevance
- CLI or Gathering page: type a word or phrase → get ranked notes back
- Zero new infrastructure. Zero disk cost. Already in the stack.

**Phase 2 — Semantic Ripple (local AI)**:
- Ollama + nomic-embed-text on primary Mac
- ChromaDB for 823 note embeddings (~10MB)
- Finds connections with no word overlap — the true ripple
- Disk: 1TB media copy completes today, freeing headroom

**Phase 3 — Expand Corpus + Graph Walk**:
- Add chorus index (28K messages) as second corpus
- Fuseki metadata (98K tracks, 44K photos) as third
- Layer 3 graph walk enriches results: note → concept → collection → traversal
- Orchestration layer merges all three into unified ripple

## #316 Intersection: Separate Cards

- **#316** = context *assembly* — making MEMORY.md dynamic for werk-init
- **This** = context *discovery* — finding related memories

They converge in Phase 3. Card this as a spike now, separate from #316.

## Bottom Line

823 notes. Already in Fuseki. Already have body text. Start with Jena text index — no model, no new storage, no new dependencies. If that produces a ripple, invest in semantic. The box in the attic is already in the house; we just need to open it.
