# Photos Harvester: Product Decisions

**From**: Wren (PM)
**To**: Kade (Engineer)
**Date**: 2026-02-18
**Re**: Your SQLite discovery brief — answers to Q1-Q5

---

## Great work on the SQLite discovery.

The GPS and face detection data are significant. Here are your answers.

---

### Q1: Person Identity Across Sources → One person, multiple source links

**Decision**: One `foaf:Person` per real person. Multiple source links.

Same pattern as Music artists. A person is a person — not "an Apple Photos person" and "a Google Photos person." Use `foaf:Person` as the base class. Each source links its data to that person. `owl:sameAs` for cross-source identity.

Silas already aligned on this in his v0.8.1 response (lazy merge strategy).

### Q2: Unnamed Face Clusters → Option B (named persons from Google first)

**Decision**: B — import named persons from Google Takeout. Skip unnamed Apple clusters for now.

Reasoning: 5,621 unnamed clusters have no value until someone names them. That's manual work with no clear payoff yet. Named persons from Google Takeout give us immediate cross-domain connections (person → photo, potentially person → music if they're also an artist).

**Future**: A (in-app naming) is a real feature for later — probably Phase 5+. Card it when we get there, not now.

### Q3: GPS Location Data → Fold into current Photos work

**Decision**: Include GPS in the current harvester. No separate card needed.

5,856 photos with GPS is data that's already there — it would be weird to ingest it and throw away the coordinates. Include location as a property on Photo resources in the RDF. Map view and location-based browsing are separate features (card those when we're ready for the UX), but the data should be in the graph now.

### Q4: Google Photos Harvest Scope → Incremental (person data first)

**Decision**: Start with Takeout person data only. Full harvest later.

Google Takeout gives us the one thing Apple Photos doesn't: named people. That's the highest-value, lowest-effort extraction. A full Google Photos harvest (duplicating the entire cloud library locally) is a bigger discussion — storage implications, dedup against Apple Photos, different metadata formats. Not now.

**Sequence**: Apple Photos SQLite (done) → Google Takeout person labels (next) → full Google harvest (later, if needed).

### Q5: Thumbnail Strategy → Option A (JXA export, run overnight)

**Decision**: A — keep JXA export for thumbnails. Let it run overnight.

It's slow but it works. Direct filesystem access (B) is fragile across macOS versions — not worth the maintenance risk. Placeholders (C) are what we have now and they're fine for browsing, but real thumbnails make the app feel alive. Deferring (D) loses momentum.

Run the JXA thumbnail export as a background batch job. ~5 hours overnight is fine. We're not in a rush on thumbnails — they're polish, not pipeline.

---

## Summary

| Question | Decision |
|----------|----------|
| Q1: Person identity | One person per real person, `foaf:Person` base |
| Q2: Unnamed clusters | Skip for now, named persons from Google first |
| Q3: GPS data | Include in current harvest |
| Q4: Google Photos scope | Takeout person data only (incremental) |
| Q5: Thumbnails | JXA export overnight |

**Your immediate path**: Ship the SQLite-based extraction (GPS + face metadata) and keep thumbnails running overnight. Google Takeout person import is the next phase.

---

— Wren
