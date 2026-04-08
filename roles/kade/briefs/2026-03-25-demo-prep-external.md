# Demo Prep: External guests — Deb Majumdar, Allu Babu Nukala, Kathy Kysar

**From:** Wren
**Date:** 2026-03-25
**Priority:** Today — demo time TBD

## Audience

Three integration veterans (Software AG / webMethods background). They understand ESBs, canonical models, data mapping, and operational integration. They brought real webMethods packages (20 packages, 1267 doc types, 834 flow services from a J&J integration platform).

## Demo Flow — Four Surfaces

### 1. Bridge (localhost:3470)
**What they see:** Live team coordination — three AI roles working, Jeff directing, messages flowing.
**Why it matters:** This is how human+AI delivery works. Not one agent, not a chatbot — a team with roles, cards, and accountability.
**Prep:** Make sure Bridge is clean, roles are active, card moves visible.

### 2. Convergence / Photos Pipeline
**What they see:** Three photo sources (Apple 24K, Takeout 102K, iPhone 54K) merged into 80K canonical photos via era-scoped merge logic. NiFi flow + SPARQL cross-graph joins.
**Why it matters:** This IS a canonical model + integration pipeline — the exact problem they solve with webMethods, done with RDF + NiFi + agents.
**Prep:** Photos page rendering, canonical counts ready, merge spec HTML available.

### 3. Borg Self-Assessment
**What they see:** 7-dimension system analysis applied to our own infrastructure.
**Why it matters:** Shows how agents assess system health — not just build, but evaluate. Prior art for tooling they'd want.
**Prep:** self-assessment.html and ontology-comparison.html in doc-catalog.

### 4. webMethods Package Reverse Engineering (#1641)
**What they see:** Agents crack open their packages, extract canonical schemas, map flow logic, produce an ICD.
**Why it matters:** This is THEIR world. 20 packages from a real J&J platform. If agents can reverse-engineer integration logic they built, that's a compelling demo.
**Prep:** 
- Research doc: product-manager/data/research/webmethods-package-anatomy.html
- Packages extracted at /tmp/wm-packages/
- Canonical doc types inventoried (18 types, 140+ supporting nouns)
- Need: Python script to extract schemas → ICD format. Kade or Silas builds this.

## Role Assignments

- **Wren:** Moderator, frames each surface, manages transitions
- **Silas:** Infrastructure story (NiFi, observability, NiFi-native extraction)
- **Kade:** Live webMethods schema extraction + convergence page

## What to Build Before Demo

1. Schema extraction script — reads node.ndf files, outputs field inventory as HTML or JSON
2. Flow service parser — reads flow.xml, produces call graph + mapping summary
3. Brief Kade + Silas on audience context

## Response Needed

Both roles: acknowledge and prep. Kade finishes #1628 then shifts to #1641 prep. Silas preps NiFi demo narrative.
