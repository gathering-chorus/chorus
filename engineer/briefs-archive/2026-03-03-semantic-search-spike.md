# Brief: Semantic Search Spike — #782

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-03-03
**Card:** #782 — Semantic search spike — LanceDB + nomic-embed-text

## Context

Silas shipped Ollama + nomic-embed-text on Bedroom Mac. Embedding endpoint is live:

```bash
curl -s http://192.168.86.242:11434/api/embeddings \
  -d '{"model": "nomic-embed-text", "prompt": "test"}' | head -c 200
```

DEC-044 (amended): Memory as layered semantic search. Phase 2 implementation.

## What to build

1. **LanceDB in Express** — `npm install @lancedb/lancedb`. Embedded (in-process), no Docker container. Store data in `data/lance/` (gitignored).

2. **Embedding pipeline** (batch script or Express route):
   - Load stories (86 TTL files from `data/pods/jeff/stories/`)
   - Load notes (823 from Fuseki via SPARQL)
   - For each: embed via `POST http://192.168.86.242:11434/api/embeddings` with `model: "nomic-embed-text"`
   - Store in LanceDB table: `{ uri, domain, title, content, vector }`
   - Track what's embedded to enable incremental updates

3. **Hybrid search endpoint** — `GET /api/search?q=...&semantic=true`:
   - Embed the query via same Ollama endpoint
   - LanceDB vector search + FTS
   - RRF merge with existing FTS5 results
   - Return enriched results with SPARQL metadata

4. **Wire into Reflect** — replace random story picks in `self-ai.handler.ts` with semantic retrieval:
   - Embed the user's prompt
   - Find top-5 most relevant stories from LanceDB
   - Use those as context instead of random selection

## Architecture

```
Express app (Library)
  → embed query via HTTP to Bedroom:11434
  → LanceDB hybrid search (in-process, data/lance/)
  → top-N URIs → SPARQL enrichment from Fuseki
  → results
```

## AC (from card)

- [ ] LanceDB table with embedded stories + notes (~1,000 docs)
- [ ] /api/search returns semantic results with RRF hybrid ranking
- [ ] Reflect uses top-5 relevant stories instead of random picks
- [ ] No new Docker containers
- [ ] Embedding pipeline runs as batch (incremental on change)

## Trust constraints

All dependencies are open-source Apache 2.0, all inference is local. No external API calls.
