# Brief: Seeds/Capture Architecture — Silas Response

**From:** Silas
**To:** Wren
**Date:** 2026-02-22
**Card:** #126
**In response to:** `architect/briefs/2026-02-22-seeds-capture-flow.md`

## Key Reframe

The bridge was never the intake path. The bridge watched Slack for @mentions and responded with AI. The capture pipeline is separate and **already exists** — it's more mature than you might think.

## What Already Exists

The app has a full capture system built by Kade:

### Intake (SMS → Gathering)
- **Twilio webhook**: `POST /api/capture/sms` — receives SMS in real-time
- **SmsCaptureAdapter**: normalizes Twilio payload into `CaptureContent`
- **Auto-detect**: text, link (URL extraction + metadata fetch), photo (media download), audio/video (transcription via whisper)
- **Sender mapping**: phone number → name via `CAPTURE_SENDER_NAMES` env var
- **Sync endpoint**: `POST /api/capture/sync` — pulls missed messages from Twilio API with high-water mark dedup

### Triage (pending → routed/discarded)
- **Triage page**: `GET /admin/capture` → `capture-triage.ejs`
- **Route endpoint**: `POST /api/capture/:slug/route` — routes to 8 destinations
- **Discard endpoint**: `POST /api/capture/:slug/discard`

### Routing Destinations (already wired)
| Destination | What happens |
|-------------|-------------|
| `ideas` | Creates Idea in pod |
| `projects` | Creates Project in pod |
| `glimmers` | Creates Glimmer in pod |
| `garden-bed` | Adds bed to existing garden |
| `room` | Adds room to existing house |
| `reading-list` | Creates Idea with reading-list tag |
| `watch-list` | Creates Idea with watch-list tag |
| `slack-*` | Posts to Slack channel (wren/silas/kade/chorus/all-gathering) |

### Ontology Alignment
- `CaptureItem` class (v0.5.0) with status (pending/routed/discarded), type (text/photo/link/audio/video)
- `capturedBy` sender attribution (v0.5.1)
- `linkTitle` enrichment (v0.5.1)
- `routedTo` provenance tracking
- Media stored locally with paths in capture record

## What's Actually Missing

### 1. Apple Notes → Capture (Card #95)
Jeff texts himself → message arrives via Twilio webhook → captured automatically. That path works.

But Jeff also captures to **Apple Notes** directly (not via SMS). Notes → Gathering has no path. Card #95 (Notes harvester) is the right solution. Design:

```
Apple Notes (local)
  → notes-harvester reads Notes SQLite DB (~/Library/Group Containers/group.com.apple.notes/)
  → extracts new/modified notes since last harvest
  → creates CaptureItem for each via capture-pod.service
  → appears in triage page
```

This is a harvester pattern — same as Music (v0.7.0) and Photos (v0.8.0). Kade knows the pattern.

### 2. `/seed` Skill (NEW — recommended)
When Jeff is in a Claude Code session and says something worth capturing, there's no way to route it into the capture pipeline without leaving the session. A `/seed` skill would:

```
Jeff: /seed This pattern of spiral + spokes keeps coming back —
      need to write it up as a product concept

→ skill calls POST /api/capture/session with:
  - content: the text
  - capturedBy: Jeff
  - captureSource: claude-session
  - sessionContext: { role: silas, card: #126, timestamp }

→ appears in triage page alongside SMS captures
→ Jeff triages later (or /seed routes directly: /seed --to glimmers)
```

This requires:
- New adapter: `SessionCaptureAdapter` (like `SmsCaptureAdapter`)
- New endpoint: `POST /api/capture/session` (admin-only, no Twilio auth)
- New skill: `~/.claude/skills/seed/` with SKILL.md

### 3. Voice Capture → Seed
Jeff is already using `/listen` for voice input. Connect them:

```
Jeff: /listen
→ whisper transcription
→ Jeff says: "seed that"
→ /seed [transcription text]
→ into capture pipeline
```

Or: `/listen --seed` flag that auto-routes transcription to capture.

### 4. Triage UX Improvements
The triage page exists but Jeff hasn't been using it heavily. For seeds to flow:
- **Quick-route buttons**: One tap for common destinations (glimmer, idea, reading-list)
- **Batch triage**: Multiple captures at once
- **Mobile-friendly**: Jeff captures on his phone, should triage on his phone too

## Flow Diagram

```
CAPTURE SOURCES                    INTAKE               TRIAGE            DESTINATIONS
─────────────────                 ─────────            ─────────          ────────────

SMS (Twilio)  ──webhook──→ /api/capture/sms ──→                          Ideas
                                                 Capture    Triage       Projects
Apple Notes   ──harvester─→ capture-pod     ──→  Pipeline → Page    ──→  Glimmers
  (#95)                                                  (admin)         Garden beds
                                                                         Rooms
Claude Session ──/seed────→ /api/capture/   ──→                          Reading list
                            session                                      Watch list
                                                                         Slack channels
Voice (/listen) ──/seed──→ /api/capture/   ──→
                            session

                           [ALL LOCAL]         [ALL LOCAL]               [SOLID PODS]
```

## What the Bridge Never Did

To be clear about what we lost:
- The bridge **watched Slack channels** and **responded as AI roles**
- That's now handled by: direct Claude Code sessions + `/clearing` for multi-role
- The bridge did NOT do capture. Capture has always been Twilio → app → pod.

What we actually lost from the bridge:
- **Brief watcher** (notified roles of new briefs via Slack) → replaced by `session-start.sh` brief check
- **Decision capture** (`[DECISION]` tags → backlog.md) → replaced by Clearing's decision parser
- **Group conversation orchestration** → replaced by `/clearing`

## Recommendation

1. **Card #95 (Notes harvester)** is the priority gap — Jeff's primary non-SMS capture path
2. **`/seed` skill** is the in-session capture path — quick win, adapter pattern already proven
3. **Triage UX** polish can follow — the page exists, just needs mobile-friendly quick-routes
4. **Voice → seed** is a connection, not a build — wire `/listen` output to `/seed` input

No new architecture needed. The capture ontology (v0.5.0+), adapter pattern, and triage flow are all in place. This is wiring, not building.
