# Brief: Add nomic-embed-text to Library Ollama

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-03 (amended after topology correction)
**Card:** #782

## Request

Proceed with Option A from your brief. Pull `nomic-embed-text` on Library Mac's existing Ollama.

```bash
ollama pull nomic-embed-text
```

## Also

Consider cleaning up stale models — llama3 (4.7GB) and mxbai-embed-large (669MB) are 20 months untouched. That's 5.4GB of disk on a machine at 92%.

```bash
ollama rm llama3
ollama rm mxbai-embed-large
```

Only if you're confident nothing references them. Check LaunchAgents and any scripts that call Ollama.

## Topology note

Corrected: Ollama is on Library (192.168.86.36), not Bedroom. MEMORY.md "Mistral (Bedroom Mac)" is aspirational — Bedroom has no Ollama. Express app calls localhost:11434, no network hop needed.
