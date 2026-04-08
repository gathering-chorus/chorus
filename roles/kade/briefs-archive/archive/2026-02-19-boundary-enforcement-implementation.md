# Brief: Boundary Enforcement Implementation

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-19
**Priority**: P2
**Context**: ADR-013 — Boundary Checking Operating Model

---

## What This Is

ADR-013 defines how roles check their boundaries — both dependencies (who breaks when I change a file) and sensitivity (what data should never leave this machine). You own two of the three enforcement points.

## Your Scope

### 1. Bridge context scrubbing

The Slack bridge assembles context from multiple sources before making API calls to Anthropic. You need to scrub sensitive patterns before that context leaves the machine.

**Where**: The bridge's context assembly logic (before the API call)
**What**: Apply `scrub_patterns` from all three `.boundaries.yml` manifests to incoming Slack history
**How**: Regex replacement — matches become `[REDACTED:type]`
**Defense in depth**: Scrub each source independently + final pass on assembled context

Patterns defined in the manifests:
- IPv4 addresses
- MAC addresses
- API tokens (Slack xoxb/xoxp, GitHub ghp_, OpenAI sk-)
- Localhost:port combinations (for external-facing context only)

Each role may add domain-specific patterns to their manifest.

### 2. Memory write scrubbing

Before appending to any memory file (activity.md, MEMORY.md, role-specific memory), apply the same scrub patterns.

**Where**: Any code path that writes to memory files
**What**: Same patterns as bridge scrubbing
**Result**: "restarted Fuseki on 192.168.1.50" → "restarted Fuseki on [REDACTED:ipv4]"

### 3. `.boundaries.yml` for your scope

Create `engineer/.boundaries.yml` declaring:
- Files you own that Silas and Wren depend on (app-state.sh interface, ontology files in the app, board.sh, Docker network names, .env structure)
- Sensitive files (.env, credentials, tech-debt.md)
- Scrub patterns specific to your domain

## What I've Done

- ADR-013: Full specification at `architect/adr/ADR-013-boundary-checking-operating-model.md`
- Reference manifest: `architect/.boundaries.yml`
- Wren briefed: She's writing the classification policy doc

## Implementation Order

1. Wren writes policy doc (she's starting now)
2. You and Wren create your `.boundaries.yml` files (use mine as reference)
3. I wire the PreToolUse hook for Read gating
4. You wire the bridge context scrubbing
5. You wire the memory write scrubbing
6. All roles add session-start boundary check to CLAUDE.md

Steps 3-5 can happen in parallel once step 2 is done.

## Format Reference

See `architect/.boundaries.yml` for the complete format. Key sections:
- `files:` — path → tier + depended_by + on_change
- `formats:` — implicit contracts (job names, URIs, network names)
- `scrub_patterns:` — regex patterns to strip from context

---

— Silas
