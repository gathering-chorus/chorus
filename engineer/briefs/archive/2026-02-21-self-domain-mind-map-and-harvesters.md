# Self Domain on Mind Map + New Harvesters

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-02-21
**Cards:** #94, #95, #96

---

## The Big Picture

Jeff shared his personal planning system today — handwritten boards and frameworks from August 2025. We harvested them into two structured files that represent the **Self domain** — the innermost ring of the concentric trust model:

- `product-manager/self-stories.md` — 15 narrative accounts organized by theme
- `product-manager/self-memories.md` — values, patterns, preferences, relationships, and the "Life's Practice" ontology

Jeff's question: **"Can I see them as leaves off the mind map?"**

Answer: yes. And this establishes an important pattern.

---

## Card #94: Self Domain on Mind Map (P1)

**What:** Stories and Memories as clickable leaf nodes hanging off Self on the mind map.

**Why this matters beyond the feature:** Right now the mind map shows ontology categories (Gathering, Cultivating, Harvesting, Reflecting) but NOT instance-level data. Music has 66k tracks — they aren't on the map. But Self is different. It's a small, curated set of deeply meaningful items. Every story is worth a leaf. This is the **first domain where the mind map goes from schema to instances.**

**Proposed structure:**
```
Self
 ├── Stories
 │    ├── Working Since Thirteen
 │    ├── Outdoor Meditation
 │    ├── Staples — 15 Incidents
 │    ├── The Library and the Boxes
 │    ├── Inside Me — Life's Practice
 │    └── ... (15 total currently)
 └── Memories
      ├── Values
      │    ├── Structure as Foundation
      │    ├── Legibility Over Hope
      │    └── ...
      ├── Patterns
      │    ├── The Spiral
      │    ├── Failure Demand vs Value Creation
      │    └── ...
      ├── Relationships
      │    ├── Aubrey
      │    ├── Ravi
      │    └── ...
      └── Life's Practice
           └── Flexibility → Mindfulness → Reflection → Learning
```

**Design questions for you:**
1. How does the mind map currently pull its node data? Static config or from the SOLID pod?
2. What's the click-through UX for a leaf node? Does it open a detail view, expand inline, or navigate to a page?
3. Should we store these as RDF resources in the pod now, or can we start with a simpler data source (JSON/markdown) and graduate to pod later?
4. The pattern we establish here will eventually apply to other small-count domains (Books, maybe Garden plants). Worth designing for reuse.

**Data is ready.** The two markdown files have structured content. I can reshape into whatever format you need.

---

## Card #95: Notes Harvester — Apple Notes / iCloud (P2)

New harvester, same pattern as Music and Photos: bulk ingest from source, refine later.

**Source:** Apple Notes (Mac/iCloud)
**Destination:** Reflecting domain (and potentially Self)
**Investigation needed:**
- AppleScript access to Notes.app
- iCloud sync behavior
- Export formats available
- Incremental fetch (new/modified since last harvest)

**Context:** Jeff's natural capture flow is SMS → Notes → triage. Notes are a primary source of raw thinking. This is where ideas live before they become anything else.

---

## Card #96: Social Posts Harvester — Facebook + LinkedIn (P2)

**Source:** Jeff's public posts on Facebook and LinkedIn
**Destination:** Reflecting domain
**Investigation needed:**
- Facebook: GDPR data download (Settings → Download Your Information) gives structured JSON
- LinkedIn: Data export (Settings → Get a copy of your data) or API
- Format normalization across platforms
- Deduplication (same content posted to both)

**Context:** These posts are things Jeff has written and shared publicly. They also serve the portfolio-as-pitch strategy — public demonstrations of thinking and expertise. The revenue/independence conversation means these become more important as outward-facing content.

---

## Priority

#94 first — it establishes the pattern AND delivers visible value to Jeff immediately. #95 and #96 are investigations that can run in parallel or later.

Let me know what you need from me on data format for #94.
