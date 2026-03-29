# Brief: Team Dashboard Page in Gathering

**From**: Wren (PM) → Kade (Engineer)
**Date**: 2026-02-21
**Card**: C#25 (Chorus board)
**Priority**: P1

## Context

Jeff asked: "Do we really need Slack?" The answer is no — if Gathering has a page that shows team state. The app is already reachable via lightlifeurbangardens.com (Cloudflare Tunnel + auth). Jeff authenticates from his phone, he's in. No Slack, no bridge, no bot token.

This replaces Slack as the mobile read path for team state. It's the first step toward removing the entire Slack subsystem (bridge, tokens, channel routing, message formatting).

## What to Build

A `/team` route in Gathering that shows:

1. **Board status** — current Now/Next/Blocked items from both boards (Gathering + Chorus). Pull from Vikunja API.
2. **Recent activity** — last 20 entries from `activity.md` or Chorus index. Who did what, when.
3. **Pending briefs** — unread briefs in each role's `briefs/` directory. Just filenames + dates.
4. **Last session summaries** — most recent `next-session.md` or standup post from each role.

## Design Constraints

- **Mobile-first.** Jeff will use this from his phone. Responsive, readable on small screens.
- **Read-only for now.** Jeff scans, doesn't act from this page. Actions stay in Claude Code sessions.
- **Fast.** Page load under 2 seconds. No heavy JS frameworks. Server-rendered EJS like the rest of the app.
- **Authenticated.** Same auth as the rest of Gathering — already handled.
- **Match existing app style.** Use gathering.css, same layout patterns as profile/mind-map pages.

## Data Sources

- Vikunja API: `http://localhost:3456/api/v1/projects/2/views/4/tasks` (Gathering), `http://localhost:3456/api/v1/projects/4/views/11/tasks` (Chorus)
- Chorus index: `~/.chorus/index.db` (SQLite FTS5) — recent messages query
- Filesystem: `briefs/` directories, `next-session.md` files, `activity.md`

## What This Unlocks

If this page works, we can deprecate:
- Slack bridge (`messages/slack-bridge/`)
- Slack bot token + channel infrastructure
- `slack-post.sh` / `slack-read.sh` scripts
- Bridge-specific hooks and formatting

That's a significant infrastructure simplification. The Clearing replaces group conversations, this page replaces the read path.

## My Recommendation

Ship the simplest version first — board status + recent activity. Add briefs and session summaries in a second pass if the first version proves useful. Don't over-design the layout; Jeff will tell you what's missing once he sees it on his phone.
