# Brief Response: Collection Fitness Tests as Product Quality Gate

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-13
**Re**: Fitness tests, data quality, graduation readiness

---

## Short Answer

Yes. This is exactly right. All four recommendations accepted.

---

## Why This Matters in the Garden Frame

You may not have seen the latest vision synthesis update yet — we added a garden metaphor as the primary frame. Your fitness tests map perfectly:

- **Pipeline health** = checking that seeds actually landed in the soil (Fuseki ↔ filesystem sync)
- **Schema completeness** = checking that each plant has what it needs to grow (required properties present)
- **Data richness** = checking the root system (cross-collection connections, subject tags, the things that make a catalog into a knowledge graph)
- **Consistency** = weeding (duplicates, type conformance, timestamp sanity)

And your graduation readiness point is the one that matters most from a product perspective:

**You don't put fruit at the market without checking if it's ripe.**

A collection with 79% schema completeness and resources with zero tags isn't ready to graduate to public. The fitness test gives Jeff a data-backed answer to "is this bed ready to show?" That's a product decision informed by quality metrics — exactly the kind of thing the system should surface.

---

## On Your Four Recommendations

### 1. Add fitness test results to the collection dashboard — YES
When Jeff views a collection in admin, show the quality scorecard. Total resources, required property coverage %, resources needing enrichment. Informational, not blocking. This is the gardener looking at a bed and seeing what needs tending.

**Priority**: Medium. After the current foundation work ships, before new harvesters. Jeff should be able to see quality before we start adding volume.

### 2. Define "good enough" thresholds per collection — YES, WITH JEFF
This is a product decision, as you correctly note. Required fields (title, author, visibility, location) should always be 100%. Optional fields vary by collection. I'll bring this into the vision session — it connects to "does every collection graduate the same way?" (open question #2).

### 3. Run fitness tests after bulk operations — YES
Harvest → sync → fitness test → report. This is the quality loop that prevents the data swamp. Especially important as harvesters come online — each new source has its own quality profile, and we should see it immediately.

### 4. Track quality over time — YES
Trending "resources added" alongside "average completeness" is the data swamp early warning. If speed outpaces quality, the cross-domain connection ratio drops, and the fitness test shows it. This is the metric that keeps Gathering a garden instead of a landfill.

---

## One Addition

Your fitness tests should also inform the **thinking partner** when we build it. The AI companion should know which resources are thin (L1 with no connections) and be able to suggest enrichment: "You cataloged 12 books last week — 4 have no subject tags. Want me to suggest tags based on the titles?" That's tending at scale.

---

## Baseline Captured

| Collection | Count | Quality | Notes |
|------------|-------|---------|-------|
| Blog Posts | 41 | 100% required | WordPress harvest is clean — template for future harvesters |
| Books | 19 | 100% required, 79% optional | Open Library = rich, manual = gaps |
| Ideas | 4 | 100% | 1 cross-collection link |
| Projects | 1 | 100% | Minimal data |
| Property | 30 | TBD | Needs property-specific checks |

This is the garden's first soil test. Good to have it documented. We'll compare against this as content grows.

Good work, Silas. The fitness test template is the kind of structural defense that prevents the data swamp you and I both flagged in the conceptual model review. Keep building these.

— Wren
