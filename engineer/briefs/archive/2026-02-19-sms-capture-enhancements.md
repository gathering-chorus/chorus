# Brief: SMS Capture Enhancements — Incremental Fetch, Media Seeds, Slack Routing

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-02-19
**Priority:** P1 — Now
**Card:** #75 (Voice capture pipeline)
**Re:** Jeff sent a voice message via SMS — capture system needs to handle it

---

## What Jeff Wants

Three enhancements to SMS capture:

### 1. Incremental Fetch (not full fetch each time)

**Current behavior:** Fetches all messages on each poll.
**Desired behavior:** Fetch only messages queued since the last fetch. Track a high-water mark (timestamp or message SID) and only pull new messages.

**Why:** As the capture volume grows, re-fetching everything is wasteful and slow. Incremental is the right pattern from the start.

### 2. Video and Voice Recordings as Seeds

**Current behavior:** SMS text and photos are captured.
**Desired behavior:** Voice messages and video recordings sent via SMS/MMS should also be captured and routed as **Seeds**.

- Store the original media file (audio/video) — do NOT discard it
- Transcribe voice messages (Whisper or similar) and include the transcript as the text content of the Seed
- Video: keep both the video file and the transcript
- The original recording has value beyond the transcript (tone, setting, expression) — it's Self domain content

**Context:** Jeff discussed wanting to record stories from his phone while walking or in the garden. This is the capture path for that.

### 3. Slack Channel Routing in Triage

**Current behavior:** "Route to..." dropdown routes to Gathering domains (Ideas, Glimmers, etc.)
**Desired behavior:** Add Slack channels as routing destinations:

- `#chorus`
- `#all-gathering`
- `#wren`
- `#kade`
- `#silas`

When routed to a Slack channel, post the capture content (text + any media link) to that channel using `slack-post.sh`.

**Why:** Jeff wants to capture a thought on his phone and have it show up in the right Slack channel without opening Slack. SMS → Triage → Route to #silas. Done.

---

## Acceptance Criteria

- [ ] Fetch only new messages since last fetch (track high-water mark)
- [ ] Voice messages captured with original audio file + transcript
- [ ] Video messages captured with original video file + transcript
- [ ] Media files stored in a durable location (not temp)
- [ ] Slack channels appear in "Route to..." dropdown
- [ ] Routing to a Slack channel posts the content to that channel
- [ ] Screenshot posted to #kade before declaring done (DEC-022)

---

— Wren
