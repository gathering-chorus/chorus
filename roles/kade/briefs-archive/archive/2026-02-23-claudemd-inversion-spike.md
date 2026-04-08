# Brief: CLAUDE.md Inversion — Dynamic Bootstrap

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-02-23
**Cards:** #252 (service registry), #254 (bootstrap inversion)

## Context

Jeff wants to invert CLAUDE.md from a static 600-line generated doc to a thin bootstrap that assembles context from the live system on session start. MEMORY.md goes away — `/chorus search` replaces it.

## What Changes For You

1. **Service registry (#252)**: I'm building a `services.json` — single source of truth for all ports, URLs, health endpoints. Your scripts and any app code that hardcodes ports (e.g., Fuseki at 3030, Loki at 3102) will read from this instead.

2. **Bootstrap inversion (#254)**: Your CLAUDE.md will shrink dramatically. Instead of static instructions about deploy workflows, test commands, etc., the bootstrap will pull live state — board cards, recent commits, current work context. The init step might use `/werk` and `/chorus` to assemble what you need.

3. **MEMORY.md deletion**: Your engineer memory file gets replaced by chorus index queries. Anything worth remembering should already be in state files, board comments, or the chorus index.

## No Action Needed Yet

Wren is sequencing this. Heads up so you know it's coming. The main thing you'd touch is validating that the dynamic bootstrap gives you enough context to work effectively — same or better than the static CLAUDE.md.

## Also

#190 (Build cycle SLAs) — the instrumentation work shipped under #247. Pre-commit timing, pre-push T1/T2 tiering (default push ~21s now), deploy phase timing, Grafana Build Health row. Check if #190 is a duplicate or if there's remaining work on your side.
