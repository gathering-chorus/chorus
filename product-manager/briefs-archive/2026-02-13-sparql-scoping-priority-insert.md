# Brief: Priority Insert — SPARQL Scoping Audit

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-13
**Priority**: High — Jeff flagged this as foundational before features or harvesters

## What Changed

Jeff identified that ADR-003's SPARQL scoping and cross-collection traversal guidance (sections 7-8) is documented intent, not verified enforcement. His exact read: this is the same pattern as the visibility gap we just fixed — architecture says one thing, code doesn't enforce it.

He's right. Before we ingest new data sources or add features, we need to verify that existing Fuseki query paths are collection-scoped and establish an enforceable pattern for new handlers.

## What This Means for the Board

A new work item slots in **after CI pipeline enforcement, before visualization tooling**:

**SPARQL collection scoping audit + enforcement pattern**
- Audit every `SparqlService.query()` call — verify graph scoping
- Flag unscoped queries that could leak across collections
- Build a helper or pattern that makes scoped queries the default
- Test coverage that catches unscoped queries in collection handlers

This is the same shape as Kade's ACL write path audit — verify what's there, fix what's wrong, make the right pattern easy to follow.

## Updated Priority Sequence

| # | Item | Status |
|---|------|--------|
| 1 | Visibility Enforcement (ADR-003) | Done |
| 2 | Pod Data Backup | In Progress (Kade) |
| 3 | Fuseki TDB2 Verification | Done (Silas) |
| 4 | CI Pipeline Enforcement | Next (Kade briefed) |
| **5** | **SPARQL Scoping Audit + Enforcement** | **New — after CI, before features** |
| 6 | Visualization Tooling (ADR-004) | After foundation |
| 7 | First External Harvester | Blocked (Jeff decision) |

## Why It Matters

When handlers serve public collection data to non-admin users (which ADR-003 now enables), an unscoped SPARQL query could return data from private collections. And when harvesters add new collection types, each new handler is a potential leak if the scoping pattern isn't established.

The principle Jeff named: "Don't build on a foundation you haven't verified." We verified ACLs before building the middleware. Now verify SPARQL scoping before building features that depend on it.

## Not Urgent Today

Kade has his current queue (backup → CI). This slots in naturally after. I've given Kade a heads-up brief so he's not surprised. I'll have the full audit scope ready for his next cycle.

— Silas
