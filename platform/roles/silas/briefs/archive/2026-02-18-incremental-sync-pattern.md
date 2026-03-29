# Brief: Incremental Sync Pattern + Data Domain Classification

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-18
**Re:** Cards #72 (Incremental Harvesting) and #73 (Search) — need architectural input

---

## Context

Jeff asked two questions this morning:
1. How do we move from "rebuild everything" harvests to incremental sync?
2. How do we add search to collections (starting with Photos)?

Before answering either, Jeff made a sharp observation: **we should first classify which datasets are static vs dynamic**, because the architecture for each is fundamentally different.

---

## Data Domain Classification (Product View)

| Domain | Type | Change Frequency | Source |
|--------|------|-----------------|--------|
| **Books** | Static | Quarterly (new purchases) | Manual entry + ISBN lookup |
| **Music** | Mostly static | Monthly (new streams/purchases) | Apple Music SQLite |
| **Movies** | Static | Rarely | Manual catalog |
| **Images** (non-Photos) | Static | Rarely | Filesystem scan |
| **Blog** | Static | Rarely (new posts) | WordPress API |
| **Property/House** | Static | Rarely | Manual entry |
| **Photos** | **Dynamic** | Weekly (new photos, edits, deletions, iCloud sync) | Apple Photos SQLite |
| **Ideas/Glimmers** | **Dynamic** | Daily (active capture) | App UI → pods |
| **Seeds** | **Dynamic** | Daily (active capture) | App UI → pods |
| **Garden** | **Dynamic** | Seasonal (living system) | TBD |
| **Journal/Notes** | **Dynamic** | Daily (if harvested) | TBD |

**Pattern:** Harvesting domains (nouns, catalogs) are mostly static. Cultivating domains (verbs, growth) are dynamic. This maps to Jeff's Harvesting = nouns / Cultivating = verbs framework.

---

## Questions for Silas

### Q1: Generic vs Domain-Specific Incremental Sync

Should incremental sync be a **generic pattern** (like Pattern B for ingestion) or **domain-specific**?

**My instinct:** Generic. Something like a `HarvestSyncService` interface that any domain implements:
- `getChangedSince(timestamp)` → returns adds, updates, deletes
- `getLastSyncTimestamp()` → reads from a sync state file
- `applyChanges(changeset)` → writes to pods + updates Fuseki

But I'm not an architect — maybe the sources are too different (SQLite vs API vs filesystem vs app UI) for a single pattern to work. That's your call.

### Q2: Change Detection Mechanism

For SQLite-backed sources (Photos, Music), change detection seems straightforward — compare modification timestamps or row counts. But:
- How do we detect **deletions**? (Photo removed from Apple Photos → orphaned data in our pods)
- How do we detect **edits**? (Photo cropped in Apple Photos → thumbnail needs regenerating)
- Should we store a hash of the source record to detect changes?

### Q3: Sync State Storage

Where does "last sync" state live?
- In the pod itself (e.g., `.gathering/photos/.sync-state.json`)?
- In Fuseki (a sync metadata graph)?
- In a standalone state file?

### Q4: Search Architecture

For search (#73), I see a graduated path:

| Level | Capability | Likely Mechanism |
|-------|-----------|-----------------|
| v1 | Date range + media type filters | SQL-like query on harvested metadata |
| v2 | Location-based filtering | GPS cluster lookup |
| v3 | Full-text across domains | Fuseki SPARQL |
| v4 | Cross-domain graph queries | Multi-GRAPH SPARQL (ADR-008) |

**Question:** Is Fuseki the right engine for all four levels, or should v1/v2 use something lighter (in-memory index, SQLite read-through) and only v3+ go through SPARQL?

### Q5: Does This Change the Ontology?

Incremental sync implies new concepts:
- `jb:lastHarvestedAt` on each item
- `jb:syncState` on each collection (last sync timestamp, item count, error count)
- `jb:tombstoned` for soft-deleted items

Do these belong in the core ontology or a separate sync metadata namespace?

---

## Data Drift & Decay (Jeff's Extension)

Jeff pushed this further: it's not just "keep up with the source." It's **knowing when your data is wrong** — even when you're not actively harvesting. Two distinct problems:

### Source Drift

The source changes but Gathering doesn't know. Our copy silently goes stale.

| Drift Type | Example | Consequence |
|-----------|---------|-------------|
| **Edit drift** | Jeff crops a photo in Apple Photos | Our thumbnail shows the old version |
| **Delete drift** | Jeff deletes a photo | Orphaned metadata + thumbnail in our pod, item appears in browse that doesn't exist |
| **Metadata drift** | Jeff favorites a song, Apple updates play counts | Our data is frozen at harvest time |
| **Schema drift** | Apple changes SQLite schema in a macOS update | Harvester breaks silently or produces partial data |
| **Enrichment drift** | ISBN lookup returned incomplete data at harvest time, publisher has since corrected it | We have permanently stale metadata that could be better |

**The risk:** Static domains are especially vulnerable. If Books was harvested once 6 months ago and never re-checked, we have no idea if the data is still accurate. "Static" doesn't mean "never wrong" — it means "changes slowly enough that we don't notice the rot."

### Internal Decay

Gathering's own data degrades over time, independent of source changes.

| Decay Type | Example | Consequence |
|-----------|---------|-------------|
| **Link rot** | External URI in an RDF triple goes dead (e.g., DBpedia link for an author) | Broken reference, dead link in UI |
| **Ontology migration gap** | Ontology evolves (v0.5 → v0.8) but old data wasn't migrated | Old items use deprecated predicates, queries return incomplete results |
| **Orphan accumulation** | Thumbnails on disk for items that no longer have metadata (or vice versa) | Wasted disk, phantom items in UI |
| **Fuseki desync** | Pod data was updated but Fuseki wasn't re-indexed | SPARQL returns stale results, UI shows wrong data |
| **Stale sync state** | Last harvest was 3 months ago, sync timestamp says "all good" | False confidence — system looks healthy but data is months old |

### Questions for Silas (Q6-Q8)

**Q6: Data Health Model**

Should we design a **data health scoring system**? Something that tells Jeff (and us) at a glance: "Photos: 98% healthy, Books: 72% healthy (stale), Music: 91% healthy." Health = freshness + completeness + consistency.

Dimensions:
- **Freshness**: How recently was this domain synced vs how often it should be?
- **Completeness**: What % of items have all expected fields populated? (e.g., photos with GPS, books with cover images)
- **Consistency**: Do pod files match Fuseki? Are there orphans in either direction?

This could be a simple dashboard metric or a structured concept in the ontology (`jb:DataHealthScore`).

**Q7: Drift Detection Pattern**

For domains backed by external sources (Photos, Music, Books), should we run periodic **drift checks** — lightweight scans that compare source record counts/hashes against our data without doing a full re-harvest? Something like:
- Weekly: "Photos SQLite has 9,733 items, we have 9,730 → 3 items drifted"
- Monthly: "Music SQLite has 66,412 tracks, we have 66,000 → 412 new since last harvest"
- On-demand: "Run drift check for Books" → compares stored ISBNs against current shelf

This is different from incremental sync — drift detection is **read-only diagnosis**, sync is **write-path correction**.

**Q8: Decay Prevention vs Decay Detection**

Two strategies, not mutually exclusive:
- **Prevention**: Design patterns that make decay impossible (e.g., Fuseki always re-indexes from pods on startup, thumbnails are always regenerated from source)
- **Detection**: Periodic health checks that find and report decay (e.g., orphan scan, link checker, schema validation)

Which should we invest in first? My instinct: prevention for new work (design it right), detection for existing data (find what's already wrong).

---

## What I'm NOT Asking

I'm not asking you to build this. I'm asking for the **architectural pattern** so when Kade builds it (probably Photos first as the test case), he has a clean design to implement against. The output I'd want: an ADR or a design brief that Kade can execute from.

---

— Wren
