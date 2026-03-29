# Brief: Doc Audit — Your Sections

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-03

## Context

Jeff noticed our spine rules for doc updates aren't being enforced. I ran a doc audit on my own sections and found 5 of 9 docs drifted. Jeff wants all three roles to audit their owned sections now.

## Your Sections (from System page badges)

- **Architecture**: C4-ARCHITECTURE, SYSTEM_ARCHITECTURE, CONCEPTUAL_MODEL, SOLID_PODS_RESEARCH, UNIFIED_INDEX_ARCHITECTURE, ONTOLOGY_STATUS, CONTENT_INGESTION_MATRIX, GLOSSARY, ARCHITECTURE_DECISIONS, infrastructure-constraints
- **Operations**: INFRASTRUCTURE, SOLID-AUTHENTICATION, PUBLIC_ACCESS, + the new HTML docs (home-cloud, log-topology, log-relatedness)
- **Architecture Decision Records**: ADR-001 through ADR-014

## What To Do

For each doc in your sections:
1. Check last modified date
2. Compare against what changed since then (deploys, infra changes, new services, config changes)
3. If stale → update it
4. If stable reference (hasn't drifted) → skip

## Known Stale Candidates

- **INFRASTRUCTURE.md** (Feb 21, 10d) — deploy times changed (121s → 24s), CSS login added, Promtail daemon scrape added, dead daemon audit done, node-exporter fixed. Significant drift.
- **SOLID-AUTHENTICATION.md** (Feb 21) — local CSS OIDC provider just shipped (#685). Auth flow is fundamentally different now.
- **SYSTEM_ARCHITECTURE.md** (Feb 21) — may need updates for new LaunchAgents, CSS provider
- **ONTOLOGY_STATUS.md** (Feb 23) — stories domain added since then?

## Card

This feeds into #763 (doc-drift gate). We're proving the audit manually today so Silas can design the automated gate with real data.
