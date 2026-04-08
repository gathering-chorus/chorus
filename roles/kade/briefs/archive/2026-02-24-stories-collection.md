# Brief: Stories Collection — Reflecting Node

**From**: Wren | **To**: Kade | **Card**: #330 | **Workflow**: WF-057 step 2
**Priority**: P1 — Kade is idle, this is ready to build

## What

Wire a Stories collection under the Reflecting branch of the mind map. Same pattern as Notes, Journal, lifes-practice — pod-backed CRUD with browse/view/create.

## Schema (Silas-confirmed, corrected namespace)

**Class**: `jb:Story` | **Collection**: `jb:StoryCollection` (subclass of `jb:Collection`)
**Graph**: `https://jeffbridwell.com/pods/jeff/stories`
**Storage**: `data/pods/jeff/stories/`
**Route**: `/collection/stories`

| Property | Type | Notes |
|----------|------|-------|
| `dcterms:title` | xsd:string | Reuse existing |
| `dcterms:created` | xsd:dateTime | When Jeff shared it |
| `jb:period` | xsd:string | New. When it happened ("1998-2002"). Free text. |
| `jb:body` | xsd:string | Already exists in ontology |
| `jb:theme` | xsd:string | New. Multi-valued. career, prior-art, management, values, identity |
| `jb:linkedDecision` | xsd:string | New. DEC-NNN references. Multi-valued. |
| `jb:storySource` | xsd:string | New. session, clearing, seed, manual |
| `jb:slug` | xsd:string | Already exists |
| `jb:hasVisibility` | jb:VisibilityLevel | Already exists. **Private by default.** |

See `architect/briefs/2026-02-24-stories-ontology-response.md` for full TTL block to add to `jb-ontology.ttl`.

## Build Scope (from Silas)

1. Add TTL block to `jb-ontology.ttl`, bump version to 1.1.0
2. Create `data/pods/jeff/stories/index.ttl` (empty container)
3. Add `StoryHandler` → `/collection/stories`
4. Add leaf to Reflecting branch in `home.ejs`
5. Add to navbar under Reflecting dropdown

## Acceptance Criteria

- [ ] `/stories` lists all stories with title, period, themes
- [ ] `/stories/:id` shows full story with linked decisions
- [ ] Create form works (manual entry for now — harvest comes later)
- [ ] Mind map shows Stories node under Reflecting
- [ ] Navbar has Stories entry
- [ ] Tests pass

## What This Enables

Once the collection exists, Wren harvests stories from session transcripts into it. The chorus index search (#318) makes them recallable. This replaces the flat `stories.md` file that's blocked by the privacy hook.

## Not In Scope

- Automated harvest from chorus index (Wren's next step after collection exists)
- AI-assisted theme tagging (Phase 2)
- Search integration (depends on #318)
