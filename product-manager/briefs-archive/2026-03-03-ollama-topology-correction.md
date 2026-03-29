# Brief: Ollama Topology Correction — #782

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-03-03
**Re:** 2026-03-03-semantic-search-ollama-setup.md

## Finding

Ollama is on **Library Mac**, not Bedroom. Bedroom has no Ollama installation at all.

```
Library (192.168.86.36):
  ollama v0.17.0
  mxbai-embed-large:latest  669 MB  (20 months old)
  llama3:latest             4.7 GB  (20 months old)

Bedroom (192.168.86.242):
  No ollama binary, no API responding on 11434
```

MEMORY.md reference to "Mistral (Bedroom Mac)" is stale/aspirational.

## Options

### A: Pull nomic-embed-text on Library (quick)
- `ollama pull nomic-embed-text` — done in 30 seconds
- Pro: No installation work, Ollama already running
- Con: Library has 16GB RAM with 18 containers. Embedding + Ollama model swap adds memory pressure. Not ideal for batch embedding jobs.
- Kade's Express app is also on Library, so localhost:11434 just works.

### B: Install Ollama on Bedroom + pull model (right)
- Bedroom has 32GB RAM, mostly idle
- Better home for AI inference long-term
- Requires: Ollama install, possibly a LaunchAgent for `ollama serve`
- Express app would need to call `http://192.168.86.242:11434` instead of localhost — network hop but on same LAN (1-3ms)

## Recommendation

**Option A now, migrate to B later.** Pull nomic-embed-text on Library so Kade can start #782 integration. The batch embedding workload is intermittent — index once, then incremental. 16GB Library can handle that. When we're ready for Reflect's Mistral or heavier AI workloads, install Ollama properly on Bedroom with a LaunchAgent.

Want me to proceed with A?
