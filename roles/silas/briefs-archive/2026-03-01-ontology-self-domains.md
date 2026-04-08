# Brief: Ontology Additions — Values, Practices, People (#590)

**From:** Wren | **To:** Silas | **Date:** 2026-03-01
**Priority:** P2

## Context

Jeff asked Wren to sort everything he's shared across sessions and identify what should graduate into the ontology. Three domains emerged as high-value, low-effort additions to the Self domain: Values, Practices, and People.

**Pod data is already written** — Wren drafted TTL instance files following existing conventions. Silas needs to:
1. Review and merge the ontology class/property additions below into `jb-ontology.ttl`
2. Validate the instance TTL files follow schema conventions
3. Add collection references to `data/pods/jeff/index.ttl`

## Pod Data Locations

- `data/pods/jeff/values/` — 10 TTL files + index (Jeff's ten cross-cultural values)
- `data/pods/jeff/practices/` — 12 TTL files + index (daily/weekly life practices)
- `data/pods/jeff/people/` — 7 TTL files + index (influence map, not contacts)

## Ontology Additions (merge into jb-ontology.ttl)

### New Classes

```turtle
# ── Values Domain ──────────────────────────────────────────

jb:Value a owl:Class ;
    rdfs:label "Personal Value" ;
    rdfs:comment "A core personal value mapped across philosophical traditions. Decision test for product and life choices." .

jb:ValueCollection a owl:Class ;
    rdfs:subClassOf jb:Collection ;
    rdfs:label "Value Collection" ;
    rdfs:comment "Container for personal values" .

# ── Practices Domain ───────────────────────────────────────

jb:Practice a owl:Class ;
    rdfs:label "Life Practice" ;
    rdfs:comment "A recurring life practice — daily, weekly, seasonal, or enduring. The ten values in action." .

jb:PracticeCollection a owl:Class ;
    rdfs:subClassOf jb:Collection ;
    rdfs:label "Practice Collection" ;
    rdfs:comment "Container for life practices" .

jb:PracticeCadence a owl:Class ;
    rdfs:label "Practice Cadence" ;
    rdfs:comment "How often a practice occurs" ;
    owl:oneOf ( jb:Daily jb:Weekly jb:Monthly jb:Quarterly jb:Annual jb:Enduring ) .

jb:Daily a jb:PracticeCadence ; rdfs:label "Daily" .
jb:Weekly a jb:PracticeCadence ; rdfs:label "Weekly" .
jb:Monthly a jb:PracticeCadence ; rdfs:label "Monthly" .
jb:Quarterly a jb:PracticeCadence ; rdfs:label "Quarterly" .
jb:Annual a jb:PracticeCadence ; rdfs:label "Annual" .
jb:Enduring a jb:PracticeCadence ; rdfs:label "Enduring" ; rdfs:comment "A practice without defined cadence — ongoing, lifelong" .

# ── People Domain ──────────────────────────────────────────

jb:Person a owl:Class ;
    rdfs:subClassOf foaf:Person ;
    rdfs:label "Person" ;
    rdfs:comment "A person in Jeff's life — not a contact but a relationship with influence, lineage, and meaning." .

jb:PersonCollection a owl:Class ;
    rdfs:subClassOf jb:Collection ;
    rdfs:label "Person Collection" ;
    rdfs:comment "Container for people in Jeff's influence map" .

jb:RelationshipType a owl:Class ;
    rdfs:label "Relationship Type" ;
    rdfs:comment "The nature of a relationship" ;
    owl:oneOf ( jb:Family jb:Friend jb:Mentor jb:Colleague jb:TeamMember jb:Companion ) .

jb:Family a jb:RelationshipType ; rdfs:label "Family" .
jb:Friend a jb:RelationshipType ; rdfs:label "Friend" .
jb:Mentor a jb:RelationshipType ; rdfs:label "Mentor" .
jb:Colleague a jb:RelationshipType ; rdfs:label "Colleague" .
jb:TeamMember a jb:RelationshipType ; rdfs:label "Team Member" .
jb:Companion a jb:RelationshipType ; rdfs:label "Companion" .
```

### New Properties

```turtle
# ── Value Properties ───────────────────────────────────────

jb:valueOrder a owl:DatatypeProperty ;
    rdfs:label "value order" ;
    rdfs:domain jb:Value ;
    rdfs:range xsd:integer ;
    rdfs:comment "Display order for the value (1-10)" .

jb:sanskritName a owl:DatatypeProperty ;
    rdfs:label "Sanskrit name" ;
    rdfs:domain jb:Value ;
    rdfs:range xsd:string ;
    rdfs:comment "Sanskrit term for this value" .

jb:buddhistConcept a owl:DatatypeProperty ;
    rdfs:label "Buddhist concept" ;
    rdfs:domain jb:Value ;
    rdfs:range xsd:string ;
    rdfs:comment "Buddhist tradition mapping for this value" .

jb:taoistConcept a owl:DatatypeProperty ;
    rdfs:label "Taoist concept" ;
    rdfs:domain jb:Value ;
    rdfs:range xsd:string ;
    rdfs:comment "Taoist tradition mapping for this value" .

jb:yogicConcept a owl:DatatypeProperty ;
    rdfs:label "Yogic concept" ;
    rdfs:domain jb:Value ;
    rdfs:range xsd:string ;
    rdfs:comment "Yogic tradition mapping for this value" .

jb:valueDescription a owl:DatatypeProperty ;
    rdfs:label "value description" ;
    rdfs:domain jb:Value ;
    rdfs:range xsd:string ;
    rdfs:comment "What this value means to Jeff" .

jb:designTest a owl:DatatypeProperty ;
    rdfs:label "design test" ;
    rdfs:domain jb:Value ;
    rdfs:range xsd:string ;
    rdfs:comment "Question to test whether a feature or decision aligns with this value" .

# ── Practice Properties ────────────────────────────────────

jb:practiceCadence a owl:ObjectProperty ;
    rdfs:label "practice cadence" ;
    rdfs:domain jb:Practice ;
    rdfs:range jb:PracticeCadence ;
    rdfs:comment "How often this practice occurs" .

jb:practiceDescription a owl:DatatypeProperty ;
    rdfs:label "practice description" ;
    rdfs:domain jb:Practice ;
    rdfs:range xsd:string ;
    rdfs:comment "What this practice involves and why it matters" .

jb:practiceSeason a owl:DatatypeProperty ;
    rdfs:label "practice season" ;
    rdfs:domain jb:Practice ;
    rdfs:range xsd:string ;
    rdfs:comment "Seasonal variation in this practice" .

jb:practiceSource a owl:DatatypeProperty ;
    rdfs:label "practice source" ;
    rdfs:domain jb:Practice ;
    rdfs:range xsd:string ;
    rdfs:comment "Where Jeff learned or developed this practice" .

jb:practiceNote a owl:DatatypeProperty ;
    rdfs:label "practice note" ;
    rdfs:domain jb:Practice ;
    rdfs:range xsd:string ;
    rdfs:comment "Additional context about this practice" .

jb:relatedValue a owl:ObjectProperty ;
    rdfs:label "related value" ;
    rdfs:domain jb:Practice ;
    rdfs:range jb:Value ;
    rdfs:comment "Values this practice expresses or cultivates" .

# ── People Properties ──────────────────────────────────────

jb:relationshipType a owl:ObjectProperty ;
    rdfs:label "relationship type" ;
    rdfs:domain jb:Person ;
    rdfs:range jb:RelationshipType ;
    rdfs:comment "Nature of the relationship" .

jb:influenceDescription a owl:DatatypeProperty ;
    rdfs:label "influence description" ;
    rdfs:domain jb:Person ;
    rdfs:range xsd:string ;
    rdfs:comment "How this person has influenced Jeff — not biography but relational meaning" .
```

## Notes for Silas

- `jb:Person` extends `foaf:Person` — same pattern as `jb:Profile`
- All People instances are `jb:Private` visibility by default (concentric trust)
- Practices link to Values via `jb:relatedValue` — cross-domain query: "which practices express Balance?"
- The `jb:designTest` property on Values is unique — it's a queryable decision framework
- I deliberately left husband and son out of the instance data until Jeff confirms he wants them in the graph. Family = private by default.
- PracticeCadence enum matches the cadences Jeff articulated (#462): daily/weekly/quarterly/annual/enduring

## What I Need

1. Review the class/property additions for schema consistency
2. Merge into `jb-ontology.ttl` (bump to v1.2.0?)
3. Add collection references to `data/pods/jeff/index.ttl`
4. Validate instance TTL files
