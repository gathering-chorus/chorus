# Design Addendum: Bridge Role-to-Role Routing

**From**: Silas (Architect)
**To**: Wren (PM), Kade (Engineer)
**Date**: 2026-02-15
**Priority**: P1
**Supersedes**: Bot-ID filter in original bridge design (line 72 of channel-monitor.ts)

---

## Requirement

Any of the 4 participants — Jeff, Wren, Silas, Kade — posts a message to a role channel, that role responds in ~30s. No exceptions. The team is a conversation.

- Jeff posts to #silas → Silas responds in ~30s
- Wren posts to #kade → Kade responds in ~30s
- Kade posts to #silas → Silas responds in ~30s
- Any role posts to #all-gathering and names a role → that role responds in ~30s

## Problem

All role messages (via `slack-post.sh`) and bridge responses post under the same Slack bot user ID (`U0AEXJU76PQ`). The original design filters out this user ID to prevent infinite loops. Side effect: role-to-role messages are invisible to the bridge.

## Design: Marker-Based Filtering

**Replace the user-ID filter with a message-marker filter.**

### How It Works

1. **Bridge-originated responses** include a marker suffix: `··bridge` (two middle dots + "bridge", appended to every bridge-posted message)
2. The channel monitor ignores any message containing `··bridge` — these are bridge outputs, not inputs
3. All other messages are processed, regardless of who sent them — Jeff, roles via `slack-post.sh`, roles via hooks

### Filter Logic (replaces line 72 in channel-monitor.ts)

```typescript
// OLD: skip messages from our bot user
// if (message.user === BOT_USER_ID) continue;

// NEW: skip only bridge-originated responses
const BRIDGE_MARKER = '··bridge';
if (message.text?.includes(BRIDGE_MARKER)) continue;
```

### Bridge Response Posting

```typescript
// When bridge posts a response, append the marker
await slack.chat.postMessage({
  channel: channelId,
  text: `${responseText}\n··bridge`
});
```

### Why This Marker

- `··` (middle dots, U+00B7) won't appear in normal messages — no false positives
- Visible in Slack for debugging but unobtrusive
- Single string check — no API calls, no metadata, no second bot token
- `slack-post.sh` never adds the marker, so role messages always pass through

## Loop Safety

| Message source | Contains marker? | Bridge processes it? |
|---|---|---|
| Jeff types in Slack | No | Yes ✓ |
| Role via `slack-post.sh` | No | Yes ✓ |
| Role via Claude Code hook | No | Yes ✓ |
| Bridge response | Yes (`··bridge`) | No — filtered ✓ |

**Infinite loop scenario**: Silas posts to #wren → bridge responds as Wren (with marker) → bridge sees marker → ignores. Loop broken.

## What Changes from Original Design

1. Remove bot-user-ID filter from channel-monitor.ts
2. Add marker check in its place
3. Append marker to all bridge-posted messages
4. No other components change — polling, rate limiting, persona loading all stay the same

## Impact

Zero new infrastructure. One filter change, one string append. The bridge goes from hearing only Jeff to hearing the whole team.

— Silas
