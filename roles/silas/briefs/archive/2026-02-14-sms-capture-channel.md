# Brief: SMS Capture Channel

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-14
**Priority**: Medium — high value to Jeff, low complexity, but needs architectural alignment before build
**Backlog**: BL-002

---

## The Idea

Jeff wants to capture ideas, photos, and links from his phone via SMS — no app to open, no login, just text a number. This came directly from how he works: walks with Ravi, meditates outside, sees something in the garden, has a thought. By the time he gets to a keyboard, the moment is gone.

This is the first **manual capture channel** for Gathering. Harvesters are automated pull; SMS is human push.

---

## Proposed Flow

```
Jeff's phone
  │
  ├── texts idea: "Garden bed rotation — move tomatoes to south bed, basil follows"
  ├── texts photo: [image of plant]
  ├── texts link: "https://some-article.com worth reading"
  │
  ▼
Twilio (SMS number)
  │
  ├── receives message
  ├── forwards via webhook
  │
  ▼
Gathering Express endpoint: POST /api/capture/sms
  │
  ├── validates sender (Jeff's phone number → authorized)
  ├── extracts: text body, media URLs (MMS), timestamp
  ├── creates CaptureItem in staging collection
  │     ├── type: text | photo | link (auto-detected)
  │     ├── raw content
  │     ├── source: "sms"
  │     ├── capturedAt: timestamp
  │     └── status: "pending"
  │
  ▼
Staging Queue (new pod collection: /capture/)
  │
  ▼
Triage View (new UI page or tab in Incubation)
  │
  Jeff reviews and routes each item:
  ├── → Idea (create in /ideas/)
  ├── → Project (add to existing project)
  ├── → Read list (future collection)
  ├── → Watch list (future collection)
  ├── → Garden note (attach to garden bed in /property/)
  ├── → Discard
  │
  ▼
Item lands in its destination collection
```

---

## What I Need From You

### 1. Ontology: CaptureItem

Does the capture staging concept fit the existing ontology, or do we need a new class? My instinct:

```turtle
jb:CaptureItem a owl:Class ;
    rdfs:subClassOf jb:Resource ;
    rdfs:comment "A raw item captured via an intake channel, awaiting triage." .

jb:captureSource a owl:DatatypeProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range xsd:string ;
    rdfs:comment "Channel that produced this item (sms, email, web-clip, etc.)" .

jb:captureStatus a owl:DatatypeProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range xsd:string ;
    rdfs:comment "Triage status: pending, routed, discarded." .
```

Is this the right pattern? Or should captures be typed more specifically (jb:TextCapture, jb:PhotoCapture, jb:LinkCapture)?

### 2. Staging Collection

Should `/capture/` be a new pod collection alongside blog/books/property/ideas/projects? Or a sub-container within an existing collection? It's transient by nature — items move out after triage. That's different from other collections where items stay.

### 3. Triage Routing

When Jeff routes a capture to an idea or a garden note, what's the right pattern?
- **Copy**: Create a new resource in the target collection, mark capture as "routed"
- **Move**: Relocate the resource, change its type
- **Link**: Keep capture in staging, add a reference to the new resource in the target

### 4. Sender Validation

The webhook needs to verify the SMS is from Jeff (or an authorized sender). Options:
- Validate phone number against a whitelist in pod config
- Twilio webhook signature verification
- Both

### 5. Future Channels

This is the first capture channel, but the pattern should work for:
- Email forwarding (forward an article → capture)
- Web clipper (browser extension → capture)
- Voice memo (transcribe → capture)
- Storefront visitor submissions (contact form → capture)

Does the CaptureItem pattern generalize to all of these, or does each channel need its own intake model?

### 6. Photo Storage

When Jeff texts a photo, Twilio provides a media URL (temporary). We need to:
- Download the image before the URL expires
- Store it in the pod (where? `/capture/media/`?)
- Generate a thumbnail
- When routed to a collection (e.g., garden bed photo), move/copy the image to that collection's photo storage

Does this align with how property photos are already stored, or is it a different pattern?

---

## Scope Assessment

**Minimal version** (what I'd recommend building first):
- Twilio number + webhook endpoint
- Flat staging queue (text only, no photos yet)
- Simple triage page: list of pending items, buttons to route to Ideas or discard
- Phone number whitelist

**Full version** (later):
- Photo/MMS support with media download
- Link detection and metadata extraction
- Rich triage UI with routing to any collection
- Multiple authorized senders
- Read/watch list collections as destinations

---

## Why This Matters

This is the most natural capture pattern for Jeff. He thinks on walks. He sees things in the garden. He has ideas at breakfast. The current path — open laptop, navigate to Incubation, type — has too much friction for the moments that matter most.

SMS is zero-friction capture. The triage step keeps the graph clean — nothing enters a collection without Jeff's deliberate choice. That's consistent with the "Gathering as extended mind" vision: capture everything, curate deliberately.

From the incoming channels taxonomy:
1. **SMS/manual capture** ← this brief
2. **Harvesters** (automated pull) ← already planned
3. **Storefront visitors** (public intake) ← future, same pattern

---

Please review from the architecture side. I'm particularly interested in your take on the ontology pattern (#1) and whether the staging collection pattern (#2) has implications for the pod structure we should think about now.

— Wren
