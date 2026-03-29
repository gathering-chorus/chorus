# Brief: SMS Capture Channel — Build Spec

**From**: Wren (PM), with architectural decisions from Silas
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: Medium — high value to Jeff, build after E2E security tests
**Backlog**: BL-002
**Depends on**: Silas adds CaptureItem to ontology (model-first)

---

## What We're Building

Jeff captures ideas, photos, and links by texting a phone number. Messages land in a staging queue. Jeff triages them into the right collection. Zero-friction intake for a person who thinks on walks.

This is the first **manual capture channel**. The pattern must generalize to email, web clipper, voice memo, and storefront visitor submissions later.

---

## Architecture (Decided)

### Ontology: Single Class, Four Properties

Silas will add these to the ontology before you build:

```turtle
jb:CaptureItem a owl:Class ;
    rdfs:subClassOf jb:Resource ;
    rdfs:comment "A raw item captured via an intake channel, awaiting triage." .

jb:captureType a owl:DatatypeProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range xsd:string ;
    rdfs:comment "Content type: text, photo, link. Detected at intake." .

jb:captureSource a owl:DatatypeProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range xsd:string ;
    rdfs:comment "Channel: sms, email, web-clip, voice, storefront." .

jb:captureStatus a owl:DatatypeProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range xsd:string ;
    rdfs:comment "Triage status: pending, routed, discarded." .

jb:routedTo a owl:ObjectProperty ;
    rdfs:domain jb:CaptureItem ;
    rdfs:range jb:Resource ;
    rdfs:comment "The resource created when this capture was triaged." .
```

One class, not subclasses. The capture is temporary scaffolding — content type is a property, not a class distinction. `jb:routedTo` links capture to its destination for provenance ("where did this idea come from?").

### Pod Structure: New Top-Level Collection

```
data/pods/jeff/
├── blog/
├── books/
├── capture/         ← new
│   ├── .meta.ttl    ← jb:Private, always
│   ├── capture-001.ttl
│   └── media/       ← downloaded photos (v2)
├── ideas/
├── projects/
└── property/
```

- Always `jb:Private` — raw captures never graduate to public
- Pattern A: one Turtle file per item (low volume, transient)
- Routed items stay as history; discarded items can be cleaned up periodically

### Triage Routing: Copy Pattern

When Jeff routes a capture:
1. Create a new resource in the target collection (a real `jb:Idea`, `jb:GardenBed` note, etc.)
2. Set `capture.jb:captureStatus = "routed"`
3. Set `capture.jb:routedTo = <new-resource-uri>`
4. Capture stays in `/capture/` as provenance

Not move (breaks URIs). Not link-only (leaves capture as canonical). The routed resource should be a proper instance of its target type.

---

## v1 Scope (Build This)

### 1. Twilio Setup

- Get a Twilio phone number
- Configure SMS webhook → `POST /api/capture/sms`
- Set monthly spend cap ($10-20) in Twilio console as circuit breaker
- Enable Advanced Opt-In spam filtering

### 2. Webhook Endpoint

```typescript
// Register alongside WordPress webhook — same pattern, separate trust boundary
app.post('/api/capture/sms',
  captureLimiter,            // 30 req/min (same as WordPress webhook)
  twilioSignatureMiddleware,  // Verify POST came from Twilio
  phoneWhitelistMiddleware,   // Verify SMS from authorized sender
  smsCaptureHandler
);
```

**No `optionalAuth`, no `adminMiddleware`.** This endpoint sits outside the SOLID OIDC auth chain — Twilio can't authenticate via SOLID. Separate trust boundary, same as the WordPress webhook.

#### Twilio Signature Verification

```typescript
import twilio from 'twilio';

const twilioSignatureMiddleware = (req, res, next) => {
  const signature = req.headers['x-twilio-signature'];
  const valid = twilio.validateRequest(
    process.env.TWILIO_AUTH_TOKEN,
    signature,
    `${process.env.BASE_URL}/api/capture/sms`,
    req.body
  );
  if (!valid) return res.status(403).json({ error: 'Invalid signature' });
  next();
};
```

#### Phone Whitelist

```typescript
const ALLOWED_NUMBERS = (process.env.CAPTURE_ALLOWED_NUMBERS || '').split(',');

const phoneWhitelistMiddleware = (req, res, next) => {
  const from = req.body.From;
  if (!ALLOWED_NUMBERS.includes(from)) {
    logger.warn('SMS from unauthorized number', { from });
    return res.status(403).json({ error: 'Unauthorized sender' });
  }
  next();
};
```

### 3. Capture Handler

On receiving a valid SMS:
1. Extract: text body (`req.body.Body`), sender (`req.body.From`), timestamp
2. Auto-detect content type:
   - Contains URL → `link`
   - Has media (`req.body.NumMedia > 0`) → `photo` (but skip download in v1)
   - Otherwise → `text`
3. Create `CaptureItem` Turtle file in `/capture/`:

```turtle
@prefix jb: <http://jeffbridwell.com/ontology#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

<#capture-{uuid}> a jb:CaptureItem ;
    jb:content "Garden bed rotation — move tomatoes to south bed" ;
    jb:captureType "text" ;
    jb:captureSource "sms" ;
    jb:captureStatus "pending" ;
    jb:capturedAt "2026-02-14T14:30:00Z"^^xsd:dateTime .
```

4. Return TwiML response (empty `<Response/>` — no reply needed)

### 4. Triage Page

New route: `GET /incubation/triage` (or a new tab in the existing Incubation page)

**Display**: List of pending CaptureItems, newest first. Show:
- Content text (or "[photo]" / "[link: url]" for other types)
- Source channel and timestamp
- Capture type badge

**Actions per item**:
- **Route to Ideas** → creates `jb:Idea` in `/ideas/`, marks capture as routed
- **Discard** → sets `captureStatus = "discarded"`

That's it for v1. Two buttons. Keep it simple.

Read captures from the filesystem (same pattern as other collection pages). No SPARQL dependency needed.

### 5. Environment Variables

```
TWILIO_ACCOUNT_SID=...
TWILIO_AUTH_TOKEN=...
TWILIO_PHONE_NUMBER=...
CAPTURE_ALLOWED_NUMBERS=+1234567890,+0987654321
```

Add to `.env.example` and document.

---

## v2 Scope (Later)

- **Photo/MMS support**: Download media from Twilio URLs (they expire), store in `/capture/media/`, copy to target collection on routing
- **Link detection**: Extract URL, fetch title/description metadata
- **Rich triage routing**: Route to any collection (Ideas, Projects, Property garden notes, read list, watch list)
- **Multiple authorized senders** (family members)
- **Read/watch list collections** as new routing destinations
- **CaptureAdapter interface** for future channels:

```typescript
interface CaptureAdapter {
  source: string;
  validateRequest(req): boolean;
  extractContent(req): CaptureContent;
}
```

---

## Key Files

| What | Where |
|------|-------|
| Webhook pattern to follow | WordPress webhook in `src/handlers/webhook.handler.ts` |
| Route registration | `src/app.ts` (register near WordPress webhook) |
| Collection page pattern | `src/handlers/collection.handler.ts` |
| Turtle file writing | `src/services/pod-write.service.ts` |
| Incubation page | `src/handlers/idea-project.handler.ts` + `views/incubation.ejs` |
| Ontology (after Silas updates) | `data/ontology/gathering.ttl` |

---

## Testing

### Unit Tests
- Twilio signature verification (valid/invalid)
- Phone whitelist (allowed/rejected)
- Content type detection (text/link/photo)
- Turtle file generation

### E2E Tests
- POST to `/api/capture/sms` without Twilio signature → 403
- POST with valid signature but wrong phone → 403
- POST with valid signature and allowed phone → capture created
- Triage: route to Ideas → idea created, capture marked routed
- Triage: discard → capture marked discarded

---

## Sequence

1. **Silas**: Add CaptureItem class + properties to ontology
2. **Kade**: Build webhook endpoint + handler
3. **Kade**: Build triage page (tab in Incubation or standalone)
4. **Jeff**: Set up Twilio account, get number, configure spend cap
5. **Test**: Jeff texts the number, triages the result

— Wren
