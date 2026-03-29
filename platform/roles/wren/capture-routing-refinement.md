# Capture & Routing Refinement

**From**: Wren (PM)
**Date**: 2026-02-15
**Status**: Draft — needs Jeff's review and Silas's architectural input
**Context**: Jeff tested SMS triage and asked "what happens after routing?" The answer today is: not enough. This doc maps the full capture-to-lifecycle pipeline across all three capture channels.

---

## The Three Capture Channels

Jeff interacts with Gathering from anywhere through three channels. Each has a different UX but should feed into the same routing and lifecycle pipeline.

| Channel | How it works | What comes in | Friction level |
|---------|-------------|---------------|----------------|
| **SMS** | Text a photo, link, or thought from phone | Text, links, photos (MMS) | Lowest — works from anywhere, no app needed |
| **Slack** | Bidirectional — route captures + async conversation with team | Text, links, formatted messages, questions, decisions | Low — already in the workflow |
| **Upload** | Photo-based capture via book-upload page | Photos + location metadata + media type | Medium — structured, richer metadata |

**Note on Slack**: Slack is unique — it's both a routing *destination* (post a capture to a channel) and the primary *conversation channel* with the team. It's the only channel that doesn't require Jeff to be in a Claude Code session. With a Slack-to-Claude bridge service, roles become truly present in Slack — Jeff messages, the role responds async. This is the same capture pattern, pointed at AI roles instead of pod collections.

### The Insight

The book upload page isn't just for books. It already has media type selectors for Book, Record, CD, Magazine. This is a **physical object capture interface** — anything you can photograph and locate in your home. Add Plant, Tool, Seed to that list and Jeff can catalog his garden shed the same way he catalogs his bookshelves.

Combined, these three channels give Jeff full mobility:
- **Walking in the garden**: Text a photo of a plant to SMS
- **At the desk**: Use the upload page to catalog records with location data
- **In a conversation**: A thought sparks — text it, it becomes a Glimmer
- **Reading Slack**: See something worth saving — route it to a collection

---

## What Happens After Routing

Today, routing creates a resource and marks the capture as routed. But each destination should have a clear post-routing lifecycle.

### Route: Ideas

| Step | What happens |
|------|-------------|
| **On route** | Creates `jb:Idea` in `/ideas/` pod. Status: `captured`. Content from capture becomes body. |
| **What's missing** | No prompt to enrich (tags, connections, related ideas). No link back to original capture. Title defaults to capture content — often a raw text or URL, not a real title. |
| **Should happen** | After routing, show a toast with "View idea" link. The idea view should show a lightweight enrichment prompt: title, tags, related ideas (optional). Keep it fast — Jeff can enrich now or later. |
| **Lifecycle from here** | Per IDEA_PROJECT_LIFECYCLE.md: Captured → Developing → Parked/Merged → Promote to Project |

### Route: Glimmer (NEW — Silas's design)

| Step | What happens |
|------|-------------|
| **On route** | Creates `jb:Glimmer` in `/glimmers/` pod. Status: `Glowing`. |
| **What's missing** | Collection and UI don't exist yet. |
| **Should happen** | Minimal creation — just content + source + timestamp. No enrichment prompt. Glimmers are intentionally lightweight. The Glimmer List is a browse view where Jeff revisits, and can Ignite (→ Idea) or let Fade. |
| **Lifecycle from here** | Glowing → Ignited (becomes Idea) or Faded (dims naturally). Faded can reignite back to Glowing. |

### Route: Reading List / Watch List

| Step | What happens |
|------|-------------|
| **On route** | Creates `jb:Idea` with a `reading-list` or `watch-list` tag. Lives in `/ideas/`. |
| **What's wrong** | These aren't really ideas. A link to an article I want to read is not an "idea" — it's a bookmark with intent. Tagging an Idea as "reading-list" is a workaround. |
| **Should happen** | Two options: (A) Keep as tagged Ideas — simple, works now. (B) Give them their own lightweight class and collection. A reading list item has: title, URL, source, status (unread/read/abandoned). Closer to a bookmark than an idea. |
| **PM recommendation** | Option A for now. Reading/Watch lists don't need their own ontology class yet. The tag-on-Idea approach works. Revisit if the volume grows or Jeff wants a dedicated browse experience. |

### Route: Projects

| Step | What happens |
|------|-------------|
| **On route** | Creates `jb:Project` in `/projects/`. Status: `active`. |
| **What's missing** | Routing directly from capture to Project skips the Idea incubation phase. This is fine for things that are clearly already projects, but it means there's no incubation trail. |
| **Should happen** | Keep as-is. Direct-to-Project routing is a power user shortcut. Jeff knows when something is already a project. |

### Route: Garden Bed / Room (Property)

| Step | What happens |
|------|-------------|
| **On route** | Adds a garden bed or room to an existing property structure. Requires selecting which garden/house. |
| **What's missing** | A photo texted via SMS of a garden bed doesn't carry location metadata. The upload page does (room/bookcase/shelf). There's a gap between "I texted a photo of my tomatoes" and "this photo belongs to the raised beds in the back garden." |
| **Should happen** | SMS captures routed to property need an enrichment step: "Which property? Which location?" The triage page could show a location picker when property destinations are selected. (This already partially works — the dropdown shows gardens/houses.) |

### Route: Slack Channels

| Step | What happens |
|------|-------------|
| **On route** | Posts capture content to the Slack channel. Fire and forget. |
| **What's missing** | No provenance. Once posted to Slack, the content lives only in Slack — it's not in the pod, not queryable, not connected to anything. |
| **Should happen** | Keep fire-and-forget. Slack is a communication channel, not a storage layer. If something posted to Slack becomes important, Jeff can recapture it. Don't over-engineer this path. |

---

## The Upload Page as General Object Capture

The book upload page is the most structured capture interface. It already handles:
- Media type selection (Book, Record, CD, Magazine)
- Location metadata (Room → Bookcase → Shelf)
- Photo capture and processing
- API enrichment (Open Library for books)

### What it could become

Rename from "Add Books" to **"Catalog"** or **"Capture"**. Expand media types:

| Media Type | Icon | Collection | Enrichment API |
|-----------|------|-----------|----------------|
| Book | existing | `/books/` | Open Library (ISBN) |
| Record | existing | `/music/` or `/media/` | Discogs, MusicBrainz |
| CD | existing | `/music/` or `/media/` | Discogs, MusicBrainz |
| Magazine | existing | `/media/` | — |
| Plant | new | `/garden/` | — (manual) |
| Tool | new | `/property/` | — (manual) |
| Seed | new | `/garden/` | — (manual) |

The location picker already works for Room → Bookcase → Shelf. For garden items, it becomes Garden → Bed → Section. Same UI pattern, different vocabulary.

### Key principle

The upload page becomes the **structured capture channel** — for when you're at the desk, have time, and want rich metadata. SMS is the **unstructured capture channel** — for when you're mobile and want zero friction. Both feed into the same pod collections and follow the same lifecycle.

---

## Cross-Channel Provenance

Every routed resource should carry a link back to how it entered the system:

```turtle
<#my-idea> jb:capturedVia jb:SMSCapture ;
    jb:capturedAt "2026-02-15T12:45:00Z"^^xsd:dateTime .

<#my-book> jb:capturedVia jb:PhotoUpload ;
    jb:capturedAt "2026-02-15T14:30:00Z"^^xsd:dateTime .
```

This matters for Jeff's self-understanding goal. "How do I capture things? When am I most generative? Do my best ideas come from walks (SMS) or from desk time (upload)?"

---

## Summary: The Capture Pattern

```
                    ┌─────────────┐
    SMS ──────────→ │             │
                    │   Triage    │──→ Idea (lifecycle: captured → developing → project)
    Slack ────────→ │   Page      │──→ Glimmer (lifecycle: glowing → ignited/faded)
                    │             │──→ Reading/Watch List (tagged idea)
    Upload ───────→ │             │──→ Project (direct shortcut)
    (Catalog)       │             │──→ Property (garden bed, room)
                    │             │──→ Slack (fire & forget)
                    └─────────────┘
                          │
                    Provenance tracked
                    (how it entered, when)
```

All roads lead through triage. The channels differ in friction and metadata richness. The destinations differ in lifecycle. But the routing pattern is one pattern.

---

## What I'd Prioritize

1. **Glimmer as a routing destination** — new concept, needs ontology + UI (Silas already designed it)
2. **Provenance links** on routed resources — lightweight, high value for self-understanding
3. **Enrichment prompts** after routing to Ideas — "view idea" toast + optional title/tags
4. **Catalog page expansion** — rename from "Add Books," add Plant/Tool/Seed media types
5. **Reading/Watch List** dedicated view — deferred, tag approach works for now

Items 1-3 are the immediate value. Item 4 is the unlock for Jeff's "mobility" vision. Item 5 can wait.

---

## Open Questions for Silas

1. Should `jb:capturedVia` be a property on every routable class, or should it live on a shared interface/mixin?
2. Does the Catalog page expansion (Plant, Tool, Seed) need new ontology classes, or can they be instances of existing classes with type tags?
3. How does the upload capture path connect to the CaptureItem pipeline? Does a photo uploaded via the Catalog page create a CaptureItem first, or does it bypass triage and go directly to the collection?

— Wren
