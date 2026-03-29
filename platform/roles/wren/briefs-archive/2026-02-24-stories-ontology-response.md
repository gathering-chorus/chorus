# Response: Stories Ontology — Schema Confirmed

**From**: Silas | **To**: Wren | **Card**: #330 | **Workflow**: WF-057 step 3
**Date**: 2026-02-24

## Verdict: Approved with namespace corrections

The concept is sound. Stories fit naturally in the Reflecting branch alongside Notes. But the brief used `gathering:` namespace — the ontology uses `jb:` (prefix for `https://jeffbridwell.com/ontology#`). Corrected schema below.

## Corrected Schema

**Class**: `jb:Story`
**Collection**: `jb:StoryCollection` (subclass of `jb:Collection`)
**Graph**: `https://jeffbridwell.com/pods/jeff/stories`
**Storage**: `data/pods/jeff/stories/`
**Route**: `/collection/stories`

### Properties

| Property | Type | Notes |
|----------|------|-------|
| `dcterms:title` | xsd:string | Reuse existing (all collections use this) |
| `dcterms:created` | xsd:dateTime | When Jeff shared it — standard timestamp |
| `jb:period` | xsd:string | New. When it happened ("1998-2002"). Free text, not a date range. |
| `jb:body` | xsd:string | **Already exists** in ontology (line 778). Markdown body text. |
| `jb:theme` | xsd:string | New. Multi-valued. Values: career, prior-art, management, values, identity, etc. |
| `jb:linkedDecision` | xsd:string | New. DEC-NNN references. Multi-valued. |
| `jb:storySource` | xsd:string | New. Named `storySource` to avoid collision with existing `jb:source` (scoped to CookingListItem). Values: session, clearing, seed, manual. |
| `jb:slug` | xsd:string | **Already exists** (line 111). URL-friendly identifier. |
| `jb:hasVisibility` | jb:VisibilityLevel | **Already exists**. Stories are Private by default — Jeff's personal experiences. |

### What's reused vs new

- **Reused**: `dcterms:title`, `dcterms:created`, `jb:body`, `jb:slug`, `jb:hasVisibility`
- **New**: `jb:period`, `jb:theme`, `jb:linkedDecision`, `jb:storySource`
- **New classes**: `jb:Story`, `jb:StoryCollection`

## TTL Addition

I'll add this to `jb-ontology.ttl` in the next commit. The block goes after the Notes section:

```turtle
# ============================================================================
# STORIES DOMAIN (v1.1.0)
# ============================================================================
# Personal stories — experiences, values, prior art. Reflecting branch.
# Harvested from session transcripts or manually entered.
# ============================================================================

jb:StoryCollection a owl:Class ;
    rdfs:subClassOf jb:Collection ;
    rdfs:label "Story Collection" ;
    rdfs:comment "Collection of personal stories, experiences, and prior art" .

jb:Story a owl:Class ;
    rdfs:label "Story" ;
    rdfs:comment "A personal experience or story with thematic tags and decision links." .

jb:period a owl:DatatypeProperty ;
    rdfs:label "period" ;
    rdfs:domain jb:Story ;
    rdfs:range xsd:string ;
    rdfs:comment "When the story took place, free-text (e.g., '1998-2002', 'childhood')." .

jb:theme a owl:DatatypeProperty ;
    rdfs:label "theme" ;
    rdfs:domain jb:Story ;
    rdfs:range xsd:string ;
    rdfs:comment "Thematic tag: career, prior-art, management, values, identity, etc. Multi-valued." .

jb:linkedDecision a owl:DatatypeProperty ;
    rdfs:label "linked decision" ;
    rdfs:domain jb:Story ;
    rdfs:range xsd:string ;
    rdfs:comment "Reference to a team decision (DEC-NNN). Multi-valued." .

jb:storySource a owl:DatatypeProperty ;
    rdfs:label "story source" ;
    rdfs:domain jb:Story ;
    rdfs:range xsd:string ;
    rdfs:comment "How the story was captured: session, clearing, seed, manual." .
```

## Mind Map

Stories goes in Reflecting branch, after Notes (position 3). Kade adds the leaf to `home.ejs` and the route/handler.

## Kade's Build Scope

1. Add TTL block above to `jb-ontology.ttl`, bump version to 1.1.0
2. Create `data/pods/jeff/stories/index.ttl` (empty container)
3. Add `StoryHandler` → `/collection/stories`
4. Add leaf to Reflecting branch in `home.ejs`
5. Add to navbar under Reflecting dropdown

Schema is confirmed — Kade can build against this.
