# Brief: SMS Capture Channel — Build

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: High — Jeff has Twilio account live, ontology is ready, build it
**Context**: Jeff set up a Twilio account with a phone number. The CaptureItem ontology is in place (v0.5.0). Wren scoped the feature (BL-002). Silas reviewed architecture. This brief is the build spec.

---

## What You're Building

A webhook endpoint that receives SMS from Jeff's phone via Twilio, creates a CaptureItem in a staging collection, and a triage page where Jeff routes items to their destination or discards them.

**Minimal version (build this):**
- Twilio webhook endpoint (text only, no photos yet)
- Staging collection (`/capture/`)
- Simple triage page: list pending items, route to Ideas or discard
- Phone number whitelist + Twilio signature verification

---

## 1. Environment Config

Twilio vars are in `.env.example`. Jeff has added real values to his `.env`. The vars:

```
TWILIO_ACCOUNT_SID=AC...
TWILIO_AUTH_TOKEN=...
TWILIO_PHONE_NUMBER=+1...
CAPTURE_ALLOWED_PHONES=+1...
```

`CAPTURE_ALLOWED_PHONES` is a comma-separated list of phone numbers allowed to submit captures (just Jeff's number for now).

**Install Twilio SDK:**
```bash
npm install twilio
```

---

## 2. Webhook Endpoint

**Route**: `POST /api/capture/sms`

**Middleware chain** (register in `app.ts` alongside the WordPress webhook):
```
POST /api/capture/sms → captureLimiter → twilioSignatureMiddleware → phoneWhitelistMiddleware → smsCaptureHandler
```

This sits OUTSIDE the SOLID auth chain — same pattern as the WordPress webhook. No `optionalAuth`, no `adminMiddleware`.

### Rate Limiter
```typescript
const captureLimiter = rateLimit({
  windowMs: 60 * 1000,  // 1 minute
  max: 30                // 30 requests per minute
});
```

### Twilio Signature Verification Middleware
```typescript
import twilio from 'twilio';

const twilioSignatureMiddleware = (req, res, next) => {
  const signature = req.headers['x-twilio-signature'] as string;
  const authToken = process.env.TWILIO_AUTH_TOKEN;
  const url = `${req.protocol}://${req.get('host')}${req.originalUrl}`;

  if (!twilio.validateRequest(authToken, signature, url, req.body)) {
    logger.warn('Invalid Twilio signature on capture endpoint');
    return res.status(403).type('text/xml').send('<Response/>');
  }
  next();
};
```

### Phone Whitelist Middleware
```typescript
const phoneWhitelistMiddleware = (req, res, next) => {
  const allowedPhones = (process.env.CAPTURE_ALLOWED_PHONES || '').split(',').map(p => p.trim());
  const from = req.body.From;

  if (!allowedPhones.includes(from)) {
    logger.warn(`SMS from unauthorized number: ${from}`);
    return res.status(403).type('text/xml').send('<Response/>');
  }
  next();
};
```

### Handler
```typescript
const smsCaptureHandler = async (req, res) => {
  const { Body, From, NumMedia } = req.body;
  const captureId = `capture-${Date.now()}`;

  // Detect type
  let captureType = 'jb:TextCapture';
  const urlPattern = /https?:\/\/[^\s]+/;
  if (urlPattern.test(Body)) {
    captureType = 'jb:LinkCapture';
  }

  // Build Turtle
  const turtle = `
@prefix jb: <https://jeffbridwell.com/ontology#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<${captureId}> a jb:CaptureItem ;
    jb:hasCaptureStatus jb:Pending ;
    jb:hasCaptureType ${captureType} ;
    jb:captureSource "sms" ;
    jb:capturedAt "${new Date().toISOString()}"^^xsd:dateTime ;
    jb:captureContent """${Body.replace(/"/g, '\\"')}""" ;
    rdfs:label "${Body.substring(0, 80).replace(/"/g, '\\"')}" .
`;

  // Write to pod via PodWriteService
  await podWriteService.writeFile(
    podId,
    `capture/${captureId}.ttl`,
    turtle,
    'text/turtle'
  );

  logger.info(`SMS capture created: ${captureId} from ${From}`);

  // Reply to Jeff
  res.type('text/xml').send(
    '<Response><Message>Captured.</Message></Response>'
  );
};
```

**Key points:**
- Response must be TwiML (XML) — Twilio expects it
- Use `PodWriteService` so Fuseki sync fires automatically
- Escape the message body for Turtle string literals
- Label is first 80 chars of body (for display in triage view)

---

## 3. Staging Collection Setup

Create the capture collection directory and `.meta.ttl`:

**File**: `data/pods/jeff/capture/.meta.ttl`
```turtle
@prefix jb: <https://jeffbridwell.com/ontology#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

<> a jb:CaptureCollection ;
    rdfs:label "Captures" ;
    jb:hasVisibility jb:Private .
```

Always private. Captures never graduate to public.

---

## 4. Triage Page

**Route**: `GET /collection/capture` (admin only, use `adminMiddleware`)

**View**: New EJS template `views/collection-capture.ejs`

Keep it simple — list of pending items with action buttons:

```
┌─────────────────────────────────────────────────┐
│ Captures (3 pending)                             │
├─────────────────────────────────────────────────┤
│ "Garden bed rotation — move tomatoes to south"   │
│ via SMS · 2 minutes ago                          │
│ [Route to Ideas] [Discard]                       │
├─────────────────────────────────────────────────┤
│ "https://some-article.com worth reading"         │
│ via SMS · 15 minutes ago                         │
│ [Route to Ideas] [Discard]                       │
├─────────────────────────────────────────────────┤
│ "Call electrician about garage outlet"            │
│ via SMS · 1 hour ago                             │
│ [Route to Ideas] [Discard]                       │
└─────────────────────────────────────────────────┘
```

**Handler reads** Turtle files from `data/pods/jeff/capture/`, filters to status `jb:Pending`, sorts by `capturedAt` descending.

### Triage API Endpoints

**Route to Ideas**: `POST /api/capture/:slug/route`
```json
{ "destination": "ideas" }
```

Handler:
1. Read the capture's Turtle file
2. Create a new `jb:Idea` in `/ideas/` with the capture content as summary, status `jb:Captured`
3. Update the capture: set `jb:hasCaptureStatus jb:Routed`, add `jb:routedTo <new-idea-uri>`
4. Return success

**Discard**: `POST /api/capture/:slug/discard`

Handler:
1. Update the capture: set `jb:hasCaptureStatus jb:Discarded`
2. Return success

Both endpoints need `apiAdminMiddleware` — admin only.

---

## 5. Registration in app.ts

```typescript
// SMS Capture Channel (outside SOLID auth chain)
app.post('/api/capture/sms',
  captureLimiter,
  twilioSignatureMiddleware,
  phoneWhitelistMiddleware,
  smsCaptureHandler
);

// Capture triage page (admin only)
app.get('/collection/capture',
  adminMiddleware(logger),
  captureCollectionHandler
);

// Capture triage actions (admin only)
app.post('/api/capture/:slug/route',
  apiLimiter,
  apiAdminMiddleware(logger),
  captureRouteHandler
);

app.post('/api/capture/:slug/discard',
  apiLimiter,
  apiAdminMiddleware(logger),
  captureDiscardHandler
);
```

---

## 6. Testing

### Unit Tests
- Twilio signature validation (valid/invalid)
- Phone whitelist (allowed/blocked)
- Capture type detection (text vs link)
- Turtle generation (proper escaping, correct properties)
- Route handler (creates idea, updates capture status)
- Discard handler (updates capture status)

### E2E Tests
- `POST /api/capture/sms` without Twilio signature → 403
- `POST /api/capture/sms` with wrong phone number → 403
- `POST /api/capture/sms` with valid signature + phone → creates capture file
- `GET /collection/capture` as admin → shows pending items
- `GET /collection/capture` as unauth → blocked
- `POST /api/capture/:slug/route` → creates idea, marks routed
- `POST /api/capture/:slug/discard` → marks discarded

For E2E, you'll need to mock the Twilio signature. The `twilio` package has `twilio.getExpectedTwilioSignature(authToken, url, params)` for generating valid test signatures.

---

## Not in Scope (v1)

- Photo/MMS support (v2 — download media, store in `/capture/media/`)
- Link metadata extraction (v2 — fetch title/description from URL)
- Routing to collections other than Ideas (v2 — property, projects, etc.)
- Multiple capture channels (v2 — email, web clipper)

---

## Key Files

- Ontology: `src/ontology/jb-ontology.ttl` (CaptureItem already added, v0.5.0)
- Route registration: `src/app.ts`
- WordPress webhook (reference pattern): look for `/api/webhook/wordpress`
- PodWriteService: `src/services/pod-write.service.ts`
- Idea creation (reference): `src/handlers/idea-project.handler.ts`
- Env config: `.env.example` (Twilio vars already added)

— Silas
