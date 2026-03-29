# Brief: SPARQL Scoping Audit — Heads Up for Next Session

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-13
**Priority**: High — foundational, before new features or harvesters
**Context**: Jeff flagged that ADR-003 sections 7-8 (SPARQL horizontal access, ontology cross-collection traversal) are documented intent, not verified enforcement. Same pattern as the visibility gap. We need to close this before building on top of it.

## What's Coming

After your current queue (pod backup → CI pipeline enforcement), the next architectural work is verifying that SPARQL queries in the codebase are actually collection-scoped. ADR-003 says "handlers must scope queries to their collection's graphs" — we need to prove they do.

## The Audit (same shape as your ACL write path audit)

### Step 1: Find every `SparqlService.query()` call

Every place in the codebase that runs a SPARQL query against Fuseki. Map each call to:
- Which handler/service calls it
- What SPARQL it runs
- Whether the query restricts to a specific named graph pattern (`GRAPH <.../pods/{podId}/{collection}/...>`)

### Step 2: Flag unscoped queries

Any query that searches across all graphs without restriction is a potential cross-collection data leak. Today this is admin-only (the dashboard SPARQL tool), so it's not a security hole yet. But when public collections exist and handlers serve non-admin users, an unscoped query could return data from private collections.

### Step 3: Enforce the pattern

Options (I'll design this, you'd build it):
- A helper or wrapper that takes a collection key and returns a scoped query — makes the right thing easy
- Test coverage that catches unscoped queries in collection handlers — makes the wrong thing hard
- At minimum: a documented pattern with code review checklist

## Why Before Harvesters

When we add external source harvesters (Google Photos, Spotify, etc.), each one creates a new collection handler that queries Fuseki. If the scoping pattern isn't established and verified before that, every new handler is a potential leak point. Better to have the pattern solid with 5 collections than to retrofit it across 12.

## Not Blocking Your Current Work

Finish pod backup and CI enforcement first. This is a heads-up so you're not surprised when it lands. I'll have the full audit scope and helper design ready for your next available cycle.

— Silas
