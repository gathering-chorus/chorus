# Brief: #1270 Relationship Depth — Code Layer

**From:** Wren
**Card:** #1270
**Priority:** After #1493 (cross-domain linker)

## Context
Design doc and data layer are done. 9 people already enriched with howWeMet, sharedContext, relationshipState. The ontology extension and UI rendering are the remaining AC items.

## What's needed

### 1. Ontology extension (~20 lines in jb-ontology.ttl)
Add the turtle declarations from `product-manager/designs/relationship-depth.md` lines 55-77:
- `jb:howWeMet`, `jb:sharedContext`, `jb:mentionedInStory` (DatatypeProperties)
- `jb:RelationshipState` class with Active/Dormant/PassedAway/Historical
- Extended `jb:RelationshipType` enum: Spouse, Parent, Child, Sibling, ExtendedFamily, Neighbor, Manager

### 2. TypeScript interfaces
Update CombinedPerson / PersonResource to include new properties.

### 3. TTL parser
Extract new properties from person TTL files — same pattern as influenceDescription.

### 4. People page rendering
- Show howWeMet, sharedContext, relationshipState when available
- Filter dropdown for relationship state
- Visual differentiation: enriched people look richer than flat harvests
- Linked stories below person card

### 5. Merge logic
Ensure harvested data (LinkedIn/Apple/Facebook) + curated enrichments merge correctly.

## Files
- Ontology: `src/ontology/jb-ontology.ttl` (lines 2160-2228)
- Design: `product-manager/designs/relationship-depth.md`
- Existing brief: `engineer/briefs/2026-03-12-relationship-depth-1270.md`
- Person TTL files: `src/ontology/people/*.ttl`
- Views: `views/collection-people.ejs`
- Handler: `src/handlers/people.handler.ts`
