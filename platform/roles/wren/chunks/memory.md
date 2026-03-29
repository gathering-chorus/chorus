# Memory — Product Context

**Chunk**: memory | **Cards**: 12 | **Sequence**: 3rd (after spine, ops)

## What We're Building

A reflecting layer — personal and shared — where memory is resonance, not retrieval. A new ripple finding an old ripple. When something enters the system, it surfaces what's semantically related across all sources: conversations, music, philosophy, decisions, stories. Not keyword match — meaning match.

## Why It Matters

Jeff's richest thinking is the least connected. Philosophy, etymology, personal stories — already gathered into Notes, conversations, briefs, stories.md. But invisible to the system and to the team. The chorus index has 28K messages and gets searched 21 times total. Because grep isn't memory.

Music is emotionally charged — it shifts mood, triggers memory, changes how Jeff shows up. When music is loaded and connected, traversing a collection becomes traversing memory itself. "Including being able to listen to it."

## The First Corpus: 823 Notes Already in Fuseki

Titles like "Practice," "Cosmos," "Reflectivity," "Encapsulation," "Sartre," "Heart/Hridya," "River," "Memory." Every note is intentional — no session chatter diluting signal. This is the densest, most personal, most "dark" corpus we have. And it's already in the house. We just need to open the box.

## Philosophical Frame

- **Assembly** (Latin *ad-simulare*, PIE *sem-*): bringing together. Same root as Gathering (Versammlung).
- **Desiring-production** (Deleuze/Guattari): desire isn't lack — it's generative. It produces connections and flows. Jeff doesn't consume music — he produces connections through it. Traversal should enable that.
- **Deterritorialization**: breaking fixed structures so new ones can emerge. Data scattered across 29 volumes isn't just a mess — it's a deterritorialized collection with potential for more fluid organization than a single catalog.
- **Body without Organs**: pure potentiality before structure constrains it. Gathering before the ontology decides what it can be.
- **Conway's Law intrapersonal**: your internal structure shapes the system you build. Jeff's values, stories, and patterns become architecture. stories.md is the proof.
- **Osmosis**: the boundary between personal and team reflecting is permeable — not push/pull, not request/response. Ideas cross the membrane because the concentration gradient is there.

## Architecture (DEC-044)

Three layers, not peers — a funnel:

| Layer | Function | Tech | Status |
|-------|----------|------|--------|
| 1. Text | Recall — fast, broad, low precision | Jena text index (Lucene in Fuseki) over 823 notes | #318 spike — Phase 1 |
| 2. Semantic | Relevance — meaning-match without word overlap | Ollama nomic-embed-text + ChromaDB | Phase 2 — earned after Phase 1 proves ripple |
| 3. Graph | Reasoning — walk relationships from what L2 surfaces | RDF/OWL + SPARQL in Fuseki | Exists, underleveraged |
| Orchestrator | The glue — fan-out query, merge, rank | TBD | Phase 3 |

**Phase 1 — Text Ripple (no AI):**
Enable Jena text index on notes collection. SPARQL full-text queries with stemming, proximity, fuzzy search, BM25 ranking. "Practice" surfaces yoga, software craft, and meditation notes. Zero new infrastructure, zero disk cost. Already in the stack.

**Phase 2 — Semantic Ripple (local AI):**
Ollama + nomic-embed-text + ChromaDB. 823 note embeddings (~10MB). Finds connections with no word overlap — the true ripple. Disk headroom freed by 1TB media copy completing.

**Phase 3 — Expand Corpus + Graph Walk:**
Add chorus index (28K messages) and Fuseki metadata (98K tracks, 44K photos). Layer 3 graph walk: note → concept → collection → traversal. Cross-layer orchestrator merges all three. Ripple visualization on a Gathering page.

## Key Decisions

- **DEC-044**: Memory as layered semantic search. Reflecting is the medium, not a quadrant.
- **DEC-045**: Participants with different constraints. Memory serves all participants — human and AI.
- **Concentric trust**: all embedding and storage stays local. Ollama, not API. Self domain content never leaves the Macs.

## The Reflecting Layer

Not a quadrant alongside Gathering, Cultivating, Harvesting. It's the **medium** they exist in. Personal reflecting (meditation, music, Deleuze) and team reflecting (what did we learn, what's working) share a permeable boundary. The insights cross because the participants are close enough to the membrane.

This is what makes Chorus different: none of the existing tools have reflecting as a first-class activity. They all do Directing, Designing, Building, Proving. None say: the thing you learned matters as much as the thing you shipped.

## Active Cards

| # | Card | Owner | P |
|---|------|-------|---|
| 318 | Spike: text ripple — Jena text index over 823 notes in Fuseki | Silas | P1 |
| 316 | Invert MEMORY.md — structured context assembly | Silas | P1 |
| 273 | Chorus context index HTTP wrapper | Silas | P1 |
| 70 | Cross-domain search | Silas | P2 |
| 278 | Mood Ring refresh pipeline | Wren | P2 |
| 195 | Seed vs Glimmer — capture taxonomy | Wren | P2 |
| 281 | Cognitive load instrumentation | Wren | P2 |
| 290 | Concentric trust model — boundaries | Wren | P2 |
| 83 | Local AI for Self domain (Ollama) | Silas | P2 |
| 74 | Self domain — convergent center of ontology | Wren | P2 |
| 124 | Claude vision auto-annotation | Kade | P2 |
| 8 | Annotation pattern — extend across collections | Kade | P1 |

## Jeff's Words

> "A new ripple trying to find an old ripple."

> "All of this is already in Gathering — yet it is not instantiated in either a memory search for me or for the team."

> "If we get all music loaded, we can explore how to traverse a data set like a memory — including being able to listen to it."

> "We are all part of Gathering and Clearing regardless of how or what we are."

> "Working with the team is a place where the boundary between me and you is pretty permeable — almost like osmosis."
