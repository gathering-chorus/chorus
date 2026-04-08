# Self Domain: People & Relationships Sketch

## The Shape

People in Jeff's life aren't a contact list — they're part of how he practices. Every recurring person in the spine is a relationship with a cadence, a domain, and a depth. The structure should reflect that.

## Concentric Rings

```
                    ┌─────────────────────────┐
                    │        OUTER             │
                    │  Professional contacts   │
                    │  Financial advisor       │
                    │  Community (swim, LLUG)  │
                    │                          │
                    │    ┌─────────────────┐   │
                    │    │     INNER       │   │
                    │    │  Jennifer       │   │
                    │    │  Paulo          │   │
                    │    │  Kathy          │   │
                    │    │  Jamie          │   │
                    │    │  Mark           │   │
                    │    │                 │   │
                    │    │  ┌───────────┐  │   │
                    │    │  │   CORE    │  │   │
                    │    │  │  Aubrey   │  │   │
                    │    │  │  Julian   │  │   │
                    │    │  │  Ravi     │  │   │
                    │    │  │  Nancy    │  │   │
                    │    │  │  Dani     │  │   │
                    │    │  │  Jeff     │  │   │
                    │    │  └───────────┘  │   │
                    │    └─────────────────┘   │
                    └─────────────────────────┘
```

### Core — daily, woven into the spine
These aren't appointments. They're the texture of every day.

| Person | Cadence | How they appear in the spine |
|--------|---------|------------------------------|
| Aubrey | daily | Evening — tea, television, wind-down. The relationship that closes the day. |
| Julian | irregular | Referenced historically (morning commute era). Present in Jeff's identity. |
| Ravi | 3x daily | Morning, midday, evening walks. Not a pet errand — a rhythm anchor. |
| Nancy Bridwell | — | Jeff's mother. Core by depth, not cadence. Family is the original ground. |
| Dani Perea | — | Core friendship. In the inner circle by significance. |

### Inner — weekly/biweekly, named in the calendar
These are learning relationships — people Jeff shows up for with regularity. Each one connects to a practice domain.

| Person | Cadence | Day/Time | Practice Connection |
|--------|---------|----------|---------------------|
| Jennifer Driscoll | MWF | 6:45am | Meditation — shared practice, Google Meet |
| Paulo Dorow | weekly | Mon 1pm | Learning / Relationship |
| Kathy Kysar | weekly | Tue 12pm | Learning / Relationship |
| Jamie Banks | biweekly | Tue 1:30pm | Learning / Relationship |
| Mark Nakib | biweekly | Sun 10am | Learning / Relationship |
| Anthe Kelly | biweekly | TBD | Acupuncture / Yoga — somatic practice |

### Outer — quarterly/occasional, contextual
Not less important — just less frequent. These relationships serve specific life functions.

| Person/Group | Cadence | Practice Connection |
|--------------|---------|---------------------|
| Financial advisor | quarterly | Financial planning |
| Community center | Mon + Fri | Exercise (swim) |
| LLUG | TBD | Learning / Professional |

## Ontology Classes

```turtle
jb:Person a owl:Class ;
    rdfs:comment "A person in Jeff's relational world" .

jb:Relationship a owl:Class ;
    rdfs:comment "A named connection between Jeff and a person — carries cadence, depth, and practice binding" .

# Properties
jb:hasPerson a owl:ObjectProperty ;
    rdfs:domain jb:Relationship ;
    rdfs:range jb:Person .

jb:ring a owl:DatatypeProperty ;
    rdfs:domain jb:Relationship ;
    rdfs:comment "Concentric depth: core, inner, outer" .

jb:cadence a owl:DatatypeProperty ;
    rdfs:domain jb:Relationship ;
    rdfs:comment "daily, MWF, weekly, biweekly, quarterly, irregular" .

jb:practiceBinding a owl:ObjectProperty ;
    rdfs:domain jb:Relationship ;
    rdfs:range jb:Practice ;
    rdfs:comment "Which practice domain this relationship connects to" .

jb:dayOfWeek a owl:DatatypeProperty ;
    rdfs:domain jb:Relationship ;
    rdfs:comment "When this relationship shows up in the calendar" .

jb:spineRole a owl:DatatypeProperty ;
    rdfs:domain jb:Relationship ;
    rdfs:comment "How this person appears in the practice spine — anchor, companion, teacher, advisor" .
```

## What This Means for Product

1. **Practice spine gets richer** — Jennifer isn't just "MWF 6:45am meditation." She's a person in the inner ring whose relationship binds to the Meditation practice with a MWF cadence. The spine page could show relationship context, not just events.

2. **Self domain has structure** — right now Self is stories and memories. Adding people gives it a relational dimension. Jeff's identity isn't solo — it's defined by who he shows up for and how.

3. **Calendar (#99, now closed) has a data source** — the relationship cadences ARE the calendar. If we ever build a calendar view, it renders from this structure.

4. **Trust model for data** — the rings map to Jeff's concentric trust model. Core = local only. Inner = maybe visible in Gathering. Outer = shareable. This is how Self domain privacy works.

## Wren's POV

The thing that strikes me: every person in the inner ring is a learning relationship. Jeff doesn't have "social" appointments — he has practice partnerships. Jennifer is meditation. Paulo, Kathy, Jamie, Mark are learning/growth. Aubrey is the daily ground. Ravi is the rhythm.

This isn't a contact list. It's a map of how Jeff stays in practice. The people ARE the practices.
