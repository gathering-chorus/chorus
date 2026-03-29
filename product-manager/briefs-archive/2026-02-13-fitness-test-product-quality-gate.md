# Brief: Collection Fitness Tests as Product Quality Gate

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-13
**Priority**: Medium — not blocking build work, but shapes how we think about ingestion quality

## Context

Jeff and I just ran the first SPARQL fitness test against his book collection. 19 books in Fuseki, queried live. The pipeline works. But the data quality varies — books enriched via Open Library have rich metadata (ISBN, page count, 8-11 subject tags). Books entered manually have gaps (The Cat Who Taught Zen has no ISBN, no publisher, no year, no page count, zero subject tags).

This matters because Jeff is spending real time cataloging. Photographing covers, entering metadata, shelving locations. That effort deserves a feedback loop that confirms the data landed correctly and completely.

## What We Built

A reusable fitness test template (`../architect/fitness-test-template.md`) with four layers:

1. **Pipeline Health** — Does Fuseki match the filesystem? Catches sync drift silently dropping resources.
2. **Schema Completeness** — Are required properties present? Per-collection scorecards showing coverage percentages.
3. **Data Richness** — Are resources connected? Subject tags, cross-collection links, visibility distribution.
4. **Consistency** — Duplicates, type conformance, timestamp sanity.

Each layer is a set of SPARQL queries that return structured results. Can be run manually, in CI, or rendered in the dashboard.

## Why This Is a Product Concern

### 1. Respecting the curation effort

When Jeff catalogs 3,000 books over weeks of work, the system should be able to answer: "All 3,019 books have titles, authors, and locations. 4 are missing ISBN. 12 have zero subject tags." That's not a technical metric — it's confirmation that the work landed. Without it, Jeff has no way to know if something silently failed halfway through a batch.

### 2. Ingestion quality varies by source

The current baseline shows this clearly:

| Source | Quality |
|--------|---------|
| Open Library enrichment | Rich — ISBN, pages, 8-11 subjects |
| Manual entry | Gaps — missing pages, subjects, sometimes ISBN |
| WordPress harvest | Complete — 100% required props on all 41 posts |

As we add harvest pipelines (Google Photos, Spotify, Apple Music), each source will have its own quality profile. The fitness test gives us a per-source quality scorecard. If a new harvester produces sparse data, we'll see it immediately.

### 3. The "data swamp" guardrail

Jeff flagged this concern earlier — he doesn't want a data lake without coherence. The Layer 3 richness checks (subject tag coverage, cross-collection connection ratio) are the early warning. If resources are being added but not connected, the fitness test shows the ratio declining. That's actionable before it becomes a problem.

### 4. Graduation readiness

A collection with 79% schema completeness and 21% of resources having zero tags isn't ready to graduate to public. The fitness test gives you a data-backed answer to "is this collection ready to show?" That's a product decision informed by data quality metrics.

## Recommendations for You

### 1. Add fitness test results to the collection dashboard

When Jeff views a collection in the admin dashboard, show a small quality scorecard: total resources, required property coverage %, resources with zero tags. Not blocking — informational. Helps Jeff prioritize which resources need enrichment.

### 2. Define "good enough" thresholds per collection

Not every collection needs the same standard. Blog posts from WordPress are harvested complete — 100% is the baseline. Books entered manually might accept 90% on optional fields. But required fields (title, author, visibility, location) should always be 100%. These thresholds are product decisions, not architecture decisions.

### 3. Run fitness tests after bulk operations

After a book cataloging session, after a WordPress sync, after a future harvester runs — the fitness test confirms nothing was dropped. Consider making this a step in the harvest pipeline: harvest → sync → fitness test → report.

### 4. Track quality over time

The fitness test baseline captured today (2026-02-13) shows 19 books, 41 blog posts, etc. As content grows, trending these numbers tells the quality story. A chart showing "books added per week" alongside "average property completeness" shows whether speed is outrunning quality.

## Current Baseline

| Collection | Count | Required Props | Notes |
|------------|-------|---------------|-------|
| Blog Posts | 41 | 100% | WordPress harvest is clean |
| Books | 19 | 100% required, 79% page count | 4 books missing optional fields |
| Ideas | 4 | 100% | 1 cross-collection link |
| Projects | 1 | 100% | Minimal data |
| Property | 30 resources | TBD | Needs property-specific checks |

## Not Blocking Anything

This is a quality-of-life improvement, not a prerequisite. Kade's current work (pod backup) is higher priority. But when he's back to feature work, wiring the fitness test into the dashboard or CI would be a good small deliverable. The queries are written — it's a rendering and automation task.

The full template is at `../architect/fitness-test-template.md`.

— Silas
