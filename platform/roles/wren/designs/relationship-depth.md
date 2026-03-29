# Relationship Depth — Design Doc (#1270)

## What "relationship depth" means for Gathering

Gathering's People collection currently has 2,257 contacts with flat labels. Six `relationshipType` values (Family, Friend, Mentor, Colleague, TeamMember, Companion) and an `influenceDescription` field used by 7 hand-curated entries. The other 2,250 are harvested names with a company and job title.

The problem isn't the count — it's that **Aubrey Haltom, Jeff's husband, looks the same as a random LinkedIn connection.**

Relationship depth isn't more categories. It's **dimensions that carry meaning.**

## The Model: Three Dimensions + Stories

### 1. How We Met (`jb:howWeMet`)
Context of the relationship's origin. Not a date — a story fragment.
- "Childhood neighbor in Crystal City, MO"
- "Fidelity — my favorite manager"
- "Morning walking companion in the neighborhood"
- "Facebook harvest — haven't identified yet"

### 2. What We Share (`jb:sharedContext`)
Activities, interests, places, experiences that define the ongoing relationship.
- "Walking, garden, daily practice"
- "Music, grief, recovery"
- "Building things — he taught me how"

### 3. Relationship State (`jb:relationshipState`)
Where the relationship is now. Enumerated:
- **Active** — current, ongoing contact
- **Dormant** — meaningful but not currently in touch
- **PassedAway** — deceased, relationship persists through memory
- **Historical** — someone from a specific era, not ongoing

### 4. Stories (links, not fields)
Stories in `stories.md` already mention people by name. The missing pipe: link the story to the person's TTL entry. When you view Aubrey on the People page, you see the stories he appears in. The stories ARE the depth — the properties just give you handles to filter and navigate.

## Enrichment Strategy

**Not interviews. Three intake paths:**

1. **Mine stories.md** — extract people mentioned, link to their TTL files, populate dimensions from what Jeff already shared. This is Wren's job, no input needed from Jeff.

2. **Prompted by proximity** — when Jeff browses People or a harvest surfaces a name, capture his 5-second reaction. One sentence = one enrichment.

3. **Triggered by harvest** — Apple Contacts (#1269), Google imports. New names spark memory. Capture at the moment of recognition.

**Priority order for enrichment:**
1. People already in stories (immediate — data exists, just needs linking)
2. Inner ring (family, daily relationships — Jeff knows these deeply)
3. Key professional relationships (Chandra, past teams — career narrative)
4. Outer ring (harvested contacts — only when Jeff encounters them)

## Ontology Changes Needed

### New properties (add to jb-ontology.ttl)
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
    owl:oneOf ( jb:Active jb:Dormant jb:PassedAway jb:Historical ) .

jb:relationshipState a owl:ObjectProperty ;
    rdfs:domain jb:Person ;
    rdfs:range jb:RelationshipState .

jb:mentionedInStory a owl:DatatypeProperty ;
    rdfs:domain jb:Person ;
    rdfs:range xsd:string ;
    rdfs:comment "Reference to a story in stories.md that mentions this person" .
```

### Extended relationship types
Add to `owl:oneOf` for `RelationshipType`:
- `jb:Spouse` — partner/husband/wife
- `jb:Parent` — mother/father
- `jb:Child` — son/daughter
- `jb:Sibling` — brother/sister
- `jb:ExtendedFamily` — in-laws, grandparents, etc.
- `jb:Neighbor` — geographic relationship
- `jb:Manager` — professional leadership relationship

Keep existing 6 types. These additions express what "Family" and "Colleague" flatten.

### Page changes
- Show `howWeMet`, `sharedContext`, `relationshipState` when available
- Filter by relationship state (Active, Dormant, etc.)
- Show linked stories below the person's card
- Visual differentiation: enriched people feel richer than flat harvests

## What Success Looks Like

Aubrey's People page entry shows: Spouse, Active, stories about the book triage and the planner discovery, "shared: family, home, adoption journey, garden." Not just "Facebook friend since 2010."

The 7 hand-curated people grow to 25+ from stories.md mining alone. No Jeff input required for that first batch. The page becomes a living map of relationships, not a contact list.
