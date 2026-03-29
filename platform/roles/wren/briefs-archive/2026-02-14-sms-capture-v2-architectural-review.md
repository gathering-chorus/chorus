# Brief: SMS Capture Channel v2 — Architectural Review

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-14
**Re**: SMS capture channel v2 scope (engineer/briefs/2026-02-14-sms-capture-channel.md)

## Summary

v1 is live and tested. Wren's v2 scope is mostly right — I'm adjusting two items and adding ontology work.

---

## Feature-by-Feature Review

### Photo/MMS — Yes, with a fail-graceful pattern

Twilio media URLs expire (hours, not minutes). Download must happen inside the webhook handler, before returning the TwiML response. The critical design point: **if the download fails, don't lose the text.**

```
Webhook receives SMS with photo
  → Try to download media (5s timeout)
  → If success: save to /capture/media/{capture-id}.{ext}, set captureMediaPath
  → If fail: create capture as text-only, set jb:mediaDownloadFailed = true
  → Either way: capture is created, Jeff gets "Captured." reply
```

Storage: `/capture/media/{capture-id}-{index}.{ext}` (index for multiple attachments). On triage routing, copy media to the target collection's photo storage — don't move.

Thumbnail generation: skip in v2. CSS scaling in the triage view is fine.

### Link Detection — Yes, lightweight

v1 already auto-detects URLs. v2 adds: fetch the page `<title>` tag on intake.

- HTTP GET with 5-second timeout
- Extract `<title>` from response HTML
- If fetch fails: keep raw URL, no title. Don't block capture creation.
- Store in `jb:linkTitle` (new ontology property — see below)

Don't attempt full article extraction, screenshots, or metadata scraping. That's a different feature (read-it-later / web clipper territory).

### Rich Triage Routing — Yes, but scoped to existing collections

Wren's brief lists routing to Ideas, Projects, Property, Read list, and Watch list. **Read/watch lists don't exist as collections yet.** Building routing destinations to non-existent collections adds UI complexity for features Jeff can't use.

**v2 routing destinations:**
- **Ideas** (existing, already works in v1)
- **Projects** (existing — needs a project picker in the triage UI. Creates a note on the project, not a new project.)
- **Property** (existing — needs a garden bed / room picker. Attaches a note to a garden bed or room.)

**Deferred to v3:**
- Read list collection (needs ontology + collection + UI first)
- Watch list collection (same)

The triage UI needs a simple collection picker: "Route to → [Ideas | Projects | Property]" with a sub-picker for the specific item when routing to Projects or Property.

### Multiple Authorized Senders — Yes, with provenance

`CAPTURE_ALLOWED_PHONES` is already comma-separated. Adding family members is config-only.

New requirement: captures from different senders should be attributable. Add `jb:capturedBy` to the ontology. The webhook handler extracts the `From` number and maps it to a name (via a simple env var mapping or config file):

```
CAPTURE_SENDER_NAMES=+1XXXXXXXXXX:Jeff,+1YYYYYYYYYY:Ravi
```

This way the triage view shows "from Jeff" or "from Ravi" instead of raw phone numbers.

### CaptureAdapter Interface — Yes, build it now

Extract the SMS-specific logic into an adapter that implements a shared interface. This is the right time — v2 adds enough complexity (photo download, link fetch) that the adapter pattern pays for itself.

```typescript
interface CaptureAdapter {
  source: string;
  validateRequest(req: Request): boolean;
  extractContent(req: Request): Promise<CaptureContent>;
}

interface CaptureContent {
  text?: string;
  mediaUrls?: string[];
  detectedType: 'text' | 'photo' | 'link';
  linkUrl?: string;
  linkTitle?: string;
  senderName?: string;
  metadata?: Record<string, string>;
}
```

The webhook handler becomes thin — validate via adapter, extract via adapter, write to pod. The pod-write logic is channel-agnostic.

---

## Ontology Additions for v2

Two new properties. I'll add these before Kade starts v2.

```turtle
jb:capturedBy a owl:DatatypeProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range xsd:string ;
    rdfs:label "captured by" ;
    rdfs:comment "Who sent this capture (mapped from phone number to name)" .

jb:linkTitle a owl:DatatypeProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range xsd:string ;
    rdfs:label "link title" ;
    rdfs:comment "Page title fetched from a link capture's URL" .
```

Everything else (`captureMediaPath`, `captureUrl`, `captureContent`) is already in v0.5.0.

---

## Revised v2 Scope

| Feature | In v2 | Notes |
|---------|:-----:|-------|
| Photo/MMS download + storage | Yes | Fail-graceful, 5s timeout, no thumbnails |
| Link title extraction | Yes | Fetch `<title>`, 5s timeout, fail-graceful |
| Route to Projects | Yes | Sub-picker for which project |
| Route to Property | Yes | Sub-picker for garden bed / room |
| Route to Read/Watch list | **No → v3** | Collections don't exist yet |
| Multiple senders + capturedBy | Yes | Env var name mapping |
| CaptureAdapter interface | Yes | Extract from SMS handler |
| Thumbnail generation | **No → v3** | CSS scaling is fine for now |

---

## Sequencing

1. **Silas**: Add `jb:capturedBy` and `jb:linkTitle` to ontology (v0.5.1)
2. **Kade**: Extract CaptureAdapter interface from v1 SMS handler
3. **Kade**: Add photo download (fail-graceful) + media storage
4. **Kade**: Add link title fetch
5. **Kade**: Add project/property routing with sub-pickers in triage UI
6. **Kade**: Add sender name mapping
7. **Test**: Jeff texts photos and links, routes to different collections

Steps 2-6 are independent enough to build in any order, but extracting the adapter first (step 2) keeps the rest clean.

— Silas
