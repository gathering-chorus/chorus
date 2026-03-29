# Brief: Relationship Depth Code Changes (#1270)

**From:** Wren | **To:** Kade | **Priority:** P2 | **Card:** #1270

## Context

I've enriched 9 people TTL files with new relationship dimension properties (`howWeMet`, `sharedContext`, `relationshipState`, `mentionedInStory`) and extended relationship types (`Parent`, `Child`, `Sibling`, `Manager`, `Spouse`). The data is in place — the code needs to catch up.

Design doc: `product-manager/designs/relationship-depth.md`

## What needs to happen

### 1. Ontology extension (`src/ontology/jb-ontology.ttl`)

Add these properties after the existing Person section (~line 2165):

```turtle
jb:howWeMet a owl:DatatypeProperty ;
    rdfs:domain jb:Person ;
    rdfs:range xsd:string ;
    rdfs:comment "How Jeff met or came to know this person — origin context" .

jb:sharedContext a owl:DatatypeProperty ;
    rdfs:domain jb:Person ;
    rdfs:range xsd:string ;
    rdfs:comment "What Jeff and this person share — activities, interests, experiences" .

jb:RelationshipState a owl:Class ;
    rdfs:label "Relationship State" ;
    owl:oneOf ( jb:Active jb:Dormant jb:PassedAway jb:Historical ) .

jb:Active a jb:RelationshipState ; rdfs:label "Active" .
jb:Dormant a jb:RelationshipState ; rdfs:label "Dormant" .
jb:PassedAway a jb:RelationshipState ; rdfs:label "Passed Away" .
jb:Historical a jb:RelationshipState ; rdfs:label "Historical" .

jb:relationshipState a owl:ObjectProperty ;
    rdfs:domain jb:Person ;
    rdfs:range jb:RelationshipState .

jb:mentionedInStory a owl:DatatypeProperty ;
    rdfs:domain jb:Person ;
    rdfs:range xsd:string ;
    rdfs:comment "Reference to a story that mentions this person" .
```

Extend `RelationshipType` `owl:oneOf` to add: `jb:Spouse`, `jb:Parent`, `jb:Child`, `jb:Sibling`, `jb:ExtendedFamily`, `jb:Neighbor`, `jb:Manager`.

### 2. TypeScript interfaces

Update `CombinedPerson` in the handler and/or `PersonResource` in the service to include:
- `howWeMet?: string`
- `sharedContext?: string`
- `relationshipState?: string`
- `mentionedInStory?: string`

### 3. TTL parsing

The self-domain service TTL parser needs to extract the new properties. Pattern matches the existing `influenceDescription` extraction.

### 4. People page view (`collection-people.ejs`)

When available, show:
- `howWeMet` as a subtle line below the name
- `sharedContext` as tags or a compact line
- `relationshipState` as a badge (especially "Passed Away" — visually distinct)
- `mentionedInStory` as a link or reference
- Filter dropdown for relationship state

### 5. Merge logic

Aubrey's file now has BOTH harvested fields (`sourceNetwork`, `connectedAt`) AND hand-curated fields (`relationshipType`, `influenceDescription`, etc.) in the same TTL. The merge-by-name logic in the handler should combine both. Verify this works — it may already since both services read the same directory.

## Files I've already modified

- `data/pods/jeff/people/aubrey-haltom.ttl` — enriched with all new properties
- `data/pods/jeff/people/mother.ttl` — enriched, type changed to Parent
- `data/pods/jeff/people/father.ttl` — enriched, type changed to Parent, state PassedAway
- `data/pods/jeff/people/haravi.ttl` — added dimensions
- `data/pods/jeff/people/ravi.ttl` — added dimensions, type to Companion

## Files I've created

- `data/pods/jeff/people/julian.ttl` — Jeff's son (Child, Active)
- `data/pods/jeff/people/chandra-bommas.ttl` — favorite manager (Manager, Dormant)
- `data/pods/jeff/people/gordon.ttl` — stepfather (Parent, Active)
- `data/pods/jeff/people/joe-cunningham.ttl` — childhood friend (Friend, Active)
- `data/pods/jeff/people/jeffs-sister.ttl` — sister (Sibling, PassedAway)

## AC check

- [x] Relationship model extended beyond Friend/Family/Colleague (data done, ontology needs formal extension)
- [ ] Ontology property for relationship context/notes (properties in data, need formal ontology declaration)
- [ ] People page shows relationship context when available (page code change needed)
- [ ] Stories that mention people link to their People entry (mentionedInStory property in data, page rendering needed)
- [x] Design doc (product-manager/designs/relationship-depth.md)
