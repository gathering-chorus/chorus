# Documentation Freshness Manifest

Last audited: 2026-03-11 by Wren (#1290)

## Staleness Legend
- **CURRENT** — Content matches system reality. No action needed.
- **STALE** — Contains outdated references, wrong structure, or missing recent changes. Needs update.
- **OBSOLETE** — Redundant with better docs or no longer relevant. Archive or consolidate.
- **STABLE** — Point-in-time record (ADR, research). Doesn't age.

---

## Key Product Docs

| Doc | Status | Last Modified | Evidence |
|-----|--------|--------------|----------|
| PRODUCT_TAXONOMY.md | CURRENT | 2026-03-11 | Three loops, 8 domains, sequences match nav-tree |
| GATHERING_VISION.md | CURRENT | 2026-03-06 | Vision, collections, revenue all aligned |
| OWNER_PERSONA.md | CURRENT | 2026-03-03 | Profile, HBDI, values match practice |
| DECISIONS.md | CURRENT | 2026-03-06 | Append-only, through DEC-067 |
| IDEA_PROJECT_LIFECYCLE.md | CURRENT | 2026-03-03 | Lifecycle states accurate, implementation status unclear |
| SYSTEM_MODEL.md | CURRENT | 2026-03-04 | Four modes, three layers, Borg principle aligned |
| COMPLETENESS_MODEL.md | CURRENT | 2026-03-06 | Twelve dimensions, evergreen framework |
| CONCEPTUAL_MODEL.md | **FIXED** | 2026-03-12 | v2 rewrite: 22 Gathering domains, 10 Chorus domains, garden metaphor, updated Mermaid diagram |

## Architecture & Engineering

| Doc | Status | Last Modified | Evidence |
|-----|--------|--------------|----------|
| C4-ARCHITECTURE.md | CURRENT | 2026-03-03 | Multi-level diagrams, service inventory accurate |
| LIVING_ARCHITECTURE.md | CURRENT | 2026-03-06 | Concentric circles, two-machine topology correct |
| ENGINEERING_HORIZONTAL.md | CURRENT | 2026-03-09 | Kade's role and feedback loops |
| GUARDRAILS.md | CURRENT | 2026-03-03 | 7-layer quality chain, Layer 2.5 added recently |
| INTERACTION_PATTERNS.md | CURRENT | 2026-03-06 | Nine patterns, spine instrumentation |
| TESTING.md | CURRENT | 2026-03-03 | 2,390+ tests, 83% coverage, thresholds accurate |
| STYLE_GUIDE.md | CURRENT | 2026-03-09 | Four tiers, CSS vocabulary, role colors |
| INFRASTRUCTURE.md | CURRENT | 2026-03-03 | Two Macs, 18 containers, ports, deploy pipeline |
| STARTUP_PROCESS.md | CURRENT | 2026-03-03 | app-state.sh, container restart, health checks |
| UNIFIED_INDEX_ARCHITECTURE.md | CURRENT | 2026-03-03 | Best-of-breed tool philosophy, harvesters |
| ONTOLOGY_STATUS.md | CURRENT | 2026-03-03 | v1.1.0, 18 active domains |
| CONTENT_INGESTION_MATRIX.md | CURRENT | 2026-03-03 | L0-L3 tiers, scale estimates |
| USER_CAPABILITIES.md | CURRENT | 2026-03-03 | Capability inventory matches reality |
| SOLID-AUTHENTICATION.md | CURRENT | 2026-03-03 | CSS flow, service tokens, Docker networking |
| SCALING_RDF_TRIPLES.md | CURRENT | 2026-03-03 | Deferred analysis, projections sound |
| BOOK_CATALOG_WORKFLOW.md | CURRENT | 2026-03-03 | Spine workflow, AirDrop, RDF schema |
| SOLID_PODS_RESEARCH.md | CURRENT | 2026-03-03 | SOLID protocol, W3C Working Group, Gathering divergence |
| EMERGENT_ARCHITECTURE_PAPER.md | CURRENT | 2026-03-04 | Draft outline, Westrum/Maslow/SDT/allostasis |
| NUDGE_BRIDGE.md | CURRENT | 2026-03-08 | Clearing/nudge round-trip protocol |
| ACCESS_CONTROL_MATRIX.md | CURRENT | 2026-03-03 | Permission matrix, ~39% coverage (needs verification) |

## STALE — Needs Update

| Doc | Status | Last Modified | Issue | Priority |
|-----|--------|--------------|-------|----------|
| ~~SITE_MAP.md~~ | **FIXED** | 2026-03-11 | Rewritten to match nav-tree.json — 7 branches, correct routing | — |
| ~~README.md~~ | **FIXED** | 2026-03-11 | Dead links removed, index matches actual files | — |
| ~~ARCHITECTURE_DECISIONS.md~~ | **FIXED** | 2026-03-11 | Added 7 decision sections (ontology, nav-tree, lint, TS gate, scripts, blast radius) | — |
| ~~infrastructure-constraints.md~~ | **FIXED** | 2026-03-11 | Disk budget updated post-music-cleanup, NFS mount documented | — |
| ~~WORDPRESS-INTEGRATION.md~~ | **FIXED** | 2026-03-12 | Verified: mu-plugin, harvest endpoint, webhook all exist. Added nav-tree context. | — |
| ~~PUBLIC_ACCESS.md~~ | **FIXED** | 2026-03-12 | Verified: tunnel scripts, cloudflared config all exist at documented paths. | — |
| ~~GALLERY-REFACTORING.md~~ | **FIXED** | 2026-03-12 | Verified: all 6 planned items still pending, doc accurate. | — |
| ~~CHORUS_SDK.md~~ | **FIXED** | 2026-03-12 | Updated message count 67K→97K. SDK actively used by board-ts. | — |

## OBSOLETE — Consolidate or Archive

| Doc | Status | Issue |
|-----|--------|-------|
| **CHORUS_README.md** | OBSOLETE | Overlaps with INTERACTION_PATTERNS and GUARDRAILS; confusing product/protocol mix |
| **GATHERING_README.md** | OBSOLETE | Redundant with C4-ARCHITECTURE; music data count inconsistent |

## ADRs (Stable — Point-in-Time Records)

| Doc | Status | Notes |
|-----|--------|-------|
| ADR-001 through ADR-014 | STABLE | Architectural decisions are inherently point-in-time. Don't age. |

## HTML Visualizations (Not Audited for Content)

AI_FOUNDATIONS.html, CHEAT_SHEET.html, HOMEOSTASIS_RESEARCH.html, INTERACTION_ARCHITECTURE.html, LOG_RELATEDNESS.html, LOG_TOPOLOGY.html, NEXT_SEQUENCE.html, SELF_PORTRAIT.html, system-model-thinking.html, TECH_STACK.html

---

## Audit Summary

| Status | Count | Pct |
|--------|-------|-----|
| CURRENT (incl. fixed) | 39 | 60% |
| STABLE (ADRs) | 14 | 21% |
| HTML (unaudited) | 10 | 15% |
| STALE (remaining) | 0 | 0% |
| OBSOLETE | 2 | 3% |
| Archived | 1 | 2% |
| **Total** | **65** | |

**Core health: 97%** (53 of 55 auditable docs are current, fixed, or stable). Only 2 obsolete candidates remain (CHORUS_README, GATHERING_README — linked from nav-tree, need nav change to archive).

---

## Enforcement Proposal

### Option: Freshness nudge at close-out

Add to `werk-init.sh --close` sequence: check `FRESHNESS_MANIFEST.md` for docs whose domain touched cards completed this session. If a doc's domain was active but the doc wasn't modified, emit a nudge:

```
nudge: SITE_MAP.md may be stale — domain "nav" changed (#1294) but doc untouched since 2026-03-03
```

**Implementation**: Map board chunk labels → doc files. When a card in chunk X moves to Done, check if the corresponding doc was modified since the card was created. If not, nudge the closing role.

This is lightweight — no gate, no block, just visibility. The role decides whether to update or acknowledge. Nudge count feeds Borg (#1286) as a doc-drift metric.
