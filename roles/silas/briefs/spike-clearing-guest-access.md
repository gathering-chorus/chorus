# Spike: Clearing Guest Access — External Party as Meeting Visitor

**Author**: Silas (Architect)
**Date**: 2026-02-21
**Card**: C#31
**Time-box**: One session

## The Idea

Invite an external party to a Clearing session. They see the conversation, can participate like a visitor at a meeting. AI roles see them, respond to them, but the guest doesn't get internal team context injected into system prompts.

## Current State

The Clearing is a localhost-only Socket.IO service:
- Express + Socket.IO on a random port
- No auth (ADR-012: localhost services unauthenticated)
- Single human identity: "Jeff" hardcoded (`transcript.add('Jeff', content)`)
- AI roles get team-internal system prompts with session context
- `cloudflared` is installed, tunnel exists (57f35c2d) serving `lightlifeurbangardens.com`

## What Needs to Change

### 1. Multi-Human Identity (Small)

**Current**: Every human message is tagged "Jeff" (server.ts:107).

**Change**: Socket.IO `connection` event gets a join handshake. Client sends `{ name, role: 'host' | 'guest' }`. Server stores per-socket identity.

```typescript
// server.ts — on connection
socket.on('join', ({ name, isHost }) => {
  socket.data.name = name;
  socket.data.isHost = isHost;
  io.emit('system', `${name} joined the session`);
});

socket.on('message', (content) => {
  const sender = socket.data.name || 'Unknown';
  const msg = transcript.add(sender, content);
  io.emit('message', msg);
});
```

### 2. Session Token Auth (Small)

**Current**: No auth. Anyone who can reach the port can connect.

**Change**: Generate a random token at launch. Share via invite link. Validate on Socket.IO handshake.

```typescript
// server.ts — at launch
const SESSION_TOKEN = crypto.randomBytes(16).toString('hex');
const inviteUrl = `${baseUrl}?token=${SESSION_TOKEN}`;

// Socket.IO middleware
io.use((socket, next) => {
  const token = socket.handshake.auth?.token || socket.handshake.query?.token;
  if (token === SESSION_TOKEN) return next();
  next(new Error('Invalid session token'));
});
```

**UX**: Jeff gets a link to share. Guest clicks link, enters their name, joins. No accounts, no passwords.

### 3. Network Exposure (Small)

**Option A: Quick Tunnel** (recommended for MVP)
```bash
cloudflared tunnel --url http://localhost:${PORT}
```
Creates a temporary `*.trycloudflare.com` URL. No DNS config, no persistent tunnel. URL is random and unguessable. Tunnel dies when session ends.

**Option B: Named Tunnel Route**
Add `clearing.lightlifeurbangardens.com` to the existing tunnel config. More polished but requires DNS setup.

**Recommendation**: Option A. Quick tunnels are perfect for ephemeral sessions. The URL is already unguessable, and the session token adds a second layer.

### 4. Guest-Safe System Prompts (Small)

**Current**: AI system prompts reference internal team context, card numbers, Chorus index, workflow states.

**Change**: When a guest is present, append a guest-awareness clause to system prompts:

```
There is an external guest in this session: {guestName}.
- Be welcoming but professional
- Don't reference internal cards, workflows, or infrastructure details unless Jeff raises them
- Treat the guest's questions and contributions with the same attention as Jeff's
- Stay concise — the guest doesn't have team context
```

The `--context` flag should still work (Jeff controls what context to inject), but AI roles should be aware that a non-team member is present.

### 5. Client UI Changes (Small)

- **Join screen**: Before chat loads, prompt for name. If `?token=` is in URL, skip host join flow (guest flow).
- **Guest badge**: New participant badge color (e.g., `#e879f9` purple-pink) for guest participants.
- **Guest messages**: Styled differently from Jeff's (different border color, not right-aligned).
- **No End Session button for guests**: Only the host can end the session.

## Architecture

```
Jeff launches Clearing with --guest flag
    ↓
Server starts on random port + generates session token
    ↓
cloudflared quick tunnel → https://random-string.trycloudflare.com
    ↓
Jeff shares: https://random-string.trycloudflare.com?token=abc123
    ↓
Guest clicks link → join screen (name entry) → enters session
    ↓
Both humans chat, AI roles respond to both
    ↓
Jeff ends session → transcript saved, tunnel closes
```

## CLI Interface

```bash
# Launch with guest access enabled
chorus/clearing/bin/clearing --guest

# Output:
# The Clearing is open at http://localhost:54321
# Guest invite link: https://random-abc.trycloudflare.com?token=a1b2c3d4...
# Share this link with your guest. The tunnel closes when the session ends.
```

## Privacy / Security

- **Session token**: 128-bit random, required for Socket.IO handshake. Brute force infeasible on a session that lasts < 1hr.
- **Quick tunnel**: URL is random (`*.trycloudflare.com`). Not discoverable. Dies with the process.
- **No internal context exposure**: Guest-safe prompt clause prevents AI from volunteering internal details.
- **Transcript**: Guest messages are recorded like all others. Indexed to Chorus with guest identity.
- **File proxy**: The `/file/*` route (server.ts:69) serves local files. When `--guest` is active, this route should be **disabled** — it's a local filesystem traversal risk over a tunnel.

## What NOT to Build

- **Multi-guest**: One guest per session is enough for MVP. Multiple guests adds moderation complexity.
- **Guest AI roles**: The guest doesn't bring their own AI. They're visiting Jeff's team meeting.
- **Persistent guest accounts**: Session tokens are ephemeral. No user database.
- **Chat history for guests**: Guest sees the session from join time forward. No replay of pre-join messages.

## Effort Estimate

| Change | Lines | Risk |
|--------|-------|------|
| Multi-human identity | ~30 | Low — Socket.IO data store is built for this |
| Session token auth | ~20 | Low — middleware pattern |
| Quick tunnel integration | ~15 | Low — `cloudflared tunnel --url` is one exec call |
| Guest-safe prompts | ~10 | Low — string append to system prompt |
| Client join screen | ~60 | Low — HTML/JS, no framework |
| Client guest styling | ~20 | Low — CSS |
| CLI `--guest` flag | ~15 | Low — bin/clearing already parses flags |
| Disable file proxy for guests | ~5 | Low |

**Total: ~175 lines. One session build.**

## Open Questions

1. **Can the guest @-mention specific AI roles?** Probably yes — same UX as Jeff.
2. **Should guest see the cost/token display?** Probably hide it — it's internal operational data.
3. **Should guest see DECISION markers?** Yes — decisions are the meeting output.
4. **Quick tunnel latency?** Cloudflare quick tunnels add ~50-100ms. Acceptable for chat. Streaming tokens may feel slightly delayed.

## Recommendation

**Build it.** The changes are small, the infrastructure exists (cloudflared + Socket.IO), and the use case is immediately valuable — Jeff can bring a collaborator, advisor, or client into a Clearing session to see the team in action. That's also a compelling demo for Chorus as a product.
