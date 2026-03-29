# Brief: SMS Capture Channel — Architectural Review

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-14
**Re**: SMS capture channel (BL-002)

## Summary

The design is sound. SMS as the first capture channel is the right choice — it's the lowest-friction intake and the pattern generalizes. Six questions answered below.

---

## 1. Ontology: CaptureItem

**Single class, not subclasses.** Wren's instinct is right.

```turtle
jb:CaptureItem a owl:Class ;
    rdfs:subClassOf jb:Resource ;
    rdfs:comment "A raw item captured via an intake channel, awaiting triage." .
```

Don't split into TextCapture / PhotoCapture / LinkCapture. The capture is temporary scaffolding — it exists only until Jeff triages it. The content type matters for display and routing, but it's a property, not a class distinction:

```turtle
jb:captureType a owl:DatatypeProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range xsd:string ;
    rdfs:comment "Content type: text, photo, link. Detected at intake." .
```

Keep `captureSource` (channel: sms, email, web-clip) and `captureStatus` (pending, routed, discarded) as Wren proposed. Add one more:

```turtle
jb:routedTo a owl:ObjectProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range jb:Resource ;
    rdfs:comment "The resource created when this capture was triaged and routed." .
```

This links the capture to its destination — useful for audit trail ("where did this idea come from?") and for the AI companion to understand provenance.

**Total ontology addition**: 1 class, 4 properties. Small.

---

## 2. Staging Collection

**New top-level pod collection: `/capture/`.** Not a sub-container.

Reasoning:
- Captures feed ALL collections (ideas, projects, property, future collections). It doesn't belong inside any one of them.
- It needs its own `.meta.ttl` → always `jb:Private`. Capture content is raw, unfiltered — it should never graduate to public.
- The visibility middleware already handles collection-level gating. A new collection gets this for free.
- Pattern A (one Turtle file per item) is fine — captures are transient and low-volume. Jeff isn't going to text 5,000 times.

The collection is transient by design, but that's not architecturally different from Ideas (which promote to Projects). Items have a lifecycle: `pending → routed/discarded`. Routed items stay in `/capture/` as history (with `jb:routedTo` pointing to the real resource). Periodic cleanup can purge discarded items.

**Pod structure after**:
```
data/pods/jeff/
├── blog/
├── books/
├── capture/         ← new
│   ├── .meta.ttl    ← jb:Private, always
│   ├── capture-001.ttl
│   └── media/       ← downloaded photos
├── ideas/
├── projects/
└── property/
```

---

## 3. Triage Routing

**Copy pattern.** Create a new resource in the target collection, mark the capture as "routed."

Not move — that changes URIs, breaks any references, and the capture collection loses provenance.

Not link-only — that leaves the capture as the canonical resource. The capture is scaffolding; the routed resource should be a proper instance of its target type (a real `jb:Idea`, a real `jb:GardenBed` note).

**The flow**:
1. Jeff reviews capture in triage view
2. Selects destination: "Route to Ideas"
3. System creates a new `jb:Idea` in `/ideas/` with the capture text as content
4. System sets `capture.jb:captureStatus = "routed"` and `capture.jb:routedTo = <new-idea-uri>`
5. Capture stays in `/capture/` as history

For photos: copy the media file from `/capture/media/` to the target collection's photo storage.

Jeff can enhance the routed resource after (add title, connect to other resources, annotate). The capture gives him the raw material; curation happens in the destination.

---

## 4. Sender Validation

**Both.** Defense in depth.

**Layer 1 — Twilio webhook signature verification**: Proves the POST came from Twilio, not someone hitting the endpoint directly. Twilio signs every webhook with your auth token. The WordPress webhook already uses this pattern (`webhookAuthMiddleware` with shared secret). Same approach:

```typescript
const twilioSignatureMiddleware = (req, res, next) => {
  const signature = req.headers['x-twilio-signature'];
  const valid = twilio.validateRequest(authToken, signature, url, req.body);
  if (!valid) return res.status(403).send('Invalid signature');
  next();
};
```

**Layer 2 — Phone number whitelist**: Proves the SMS came from Jeff (or an authorized sender), not a random number texting the Twilio number. Store the whitelist in pod config (a simple Turtle file or env var).

**Why both**: Signature verification prevents endpoint abuse from outside Twilio. Phone whitelist prevents unauthorized SMS from going through Twilio. Either alone has a gap.

**Auth chain note**: This endpoint sits outside the SOLID OIDC auth chain — same as the WordPress webhook. Twilio can't authenticate via SOLID. The endpoint needs its own middleware stack:

```
POST /api/capture/sms → rateLimiter → twilioSignatureMiddleware → phoneWhitelistCheck → handler
```

No `optionalAuth`, no `adminMiddleware`. Separate trust boundary.

**Layer 3 — Twilio spend cap**: Even with both layers above, Twilio charges per inbound message before our code runs. If the number leaks and gets spammed, the bill runs up regardless of whether the whitelist rejects the messages. Set a monthly spend cap in the Twilio console (e.g., $10-20/month is plenty for personal SMS capture). This is the circuit breaker — if something unexpected happens, Twilio stops accepting messages at the cap rather than running up charges.

**Additional hardening** (low effort, do at setup):
- Enable Twilio's Advanced Opt-In spam filtering — blocks known spam senders before the webhook fires
- Keep the Twilio number private — it's a capture tool, not a contact number
- Log rejected messages (failed whitelist) for awareness — if the count spikes, the number may have leaked

---

## 5. Future Channels

**CaptureItem generalizes to all of these.** That's the right design.

Every channel produces the same thing:
- Raw content (text, image, URL)
- Source channel identifier
- Timestamp
- Content type

The channel-specific part is the **adapter** — the intake endpoint that receives from the specific service:

| Channel | Adapter | Intake |
|---------|---------|--------|
| SMS | Twilio webhook | `POST /api/capture/sms` |
| Email | IMAP poll or forwarding webhook | `POST /api/capture/email` |
| Web clipper | Browser extension POST | `POST /api/capture/web-clip` |
| Voice memo | Transcription service webhook | `POST /api/capture/voice` |
| Storefront | Contact form POST | `POST /api/capture/storefront` |

Each adapter normalizes its input into a `CaptureItem` and writes it to `/capture/`. The staging queue and triage UI are channel-agnostic — they just show pending CaptureItems regardless of source.

**Design the adapter interface now, even though we're only building SMS.** Something like:

```typescript
interface CaptureAdapter {
  source: string;           // 'sms', 'email', etc.
  validateRequest(req): boolean;  // channel-specific auth
  extractContent(req): CaptureContent;  // normalize to common format
}
```

This prevents the SMS adapter from becoming a one-off that doesn't generalize.

---

## 6. Photo Storage

**Download immediately to `/capture/media/`.** Twilio media URLs expire — you have a window to grab them.

```
Twilio webhook arrives with mediaUrl
  → download image to /capture/media/{capture-id}-{filename}
  → store local path in CaptureItem TTL
  → generate thumbnail (optional, could defer)
```

**On triage routing**: Copy the image to the target collection's photo storage. Property already has a photo pattern — follow it. Each collection owns its media; there's no unified media store.

**Don't over-engineer thumbnails in v1.** The triage view can display the full image scaled down via CSS. Real thumbnail generation is a nice-to-have for later.

**Storage concern**: If Jeff starts texting a lot of photos, `/capture/media/` grows. Add a cleanup step: when a capture is discarded, delete its media. When routed, media is copied to the target — the capture's copy can be cleaned up after a retention period.

---

## Additional Architectural Notes

### Endpoint registration

The capture endpoint should be registered alongside the WordPress webhook — same pattern, separate trust boundary:

```typescript
app.post('/api/capture/sms',
  captureLimiter,
  twilioSignatureMiddleware,
  phoneWhitelistMiddleware,
  smsCapturHandler
);
```

Rate limit recommendation: 30 requests / 1 minute (same as WordPress webhook). Jeff isn't going to text 30 times in a minute. If someone is hitting the endpoint that fast, it's abuse.

### Fuseki sync

CaptureItems should sync to Fuseki like any other pod resource. This means the triage view could use SPARQL:

```sparql
SELECT ?capture ?text ?source ?date WHERE {
  ?capture a jb:CaptureItem ;
    jb:captureStatus "pending" ;
    jb:captureSource ?source ;
    jb:capturedAt ?date .
  OPTIONAL { ?capture jb:content ?text }
}
ORDER BY DESC(?date)
```

But the triage page could also just read Turtle from the filesystem — same pattern as other collection pages. Don't add SPARQL dependency for this if filesystem reads work.

### Conceptual model update

When this moves to build, the conceptual model needs a "Capture Channel" section. The concept is already in the glossary (we added it during the v1 update). The model needs the lifecycle: capture → stage → triage → route/discard.

---

## Scope Agreement

Wren's minimal version is the right starting scope:
- Twilio number + webhook endpoint
- Text only (no photos in v1)
- Simple triage page: list pending, route to Ideas or discard
- Phone number whitelist + Twilio signature verification

Photos, link detection, and rich routing come in v2.

**Before Kade builds**: I'll add the CaptureItem class and properties to the ontology. Model-first, then build.

— Silas
