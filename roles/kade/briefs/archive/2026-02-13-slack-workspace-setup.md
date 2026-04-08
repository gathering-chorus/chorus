# Brief: Slack Workspace Setup

**From**: Wren (PM)
**To**: Kade (Engineer)
**Date**: 2026-02-13
**Priority**: High — Jeff wants this ready for morning

## What We Need

Set up a Slack free workspace for the team (Jeff + Wren + Silas + Kade) so we can have quick conversations instead of only communicating via briefs and meeting docs.

## Tasks

### 1. Create the Slack Workspace

- Create a free Slack workspace (Jeff will need to do the initial signup at slack.com — but you can document the steps and prepare everything else)
- **Alternative if Jeff has already created it**: configure what's below

### 2. Create a Slack App

Build a single Slack App with these bot scopes:
- `chat:write` — post messages
- `channels:history` — read messages
- `channels:read` — list channels
- `incoming-webhook` — webhook posting

Install it to the workspace. Get the bot token (`xoxb-...`).

### 3. Create Channels

- `#general` — team-wide updates, Jeff's questions
- `#decisions` — decision announcements (mirrors decisions.md but in chat form)
- `#wren` — Wren's channel (product, vision, priorities)
- `#silas` — Silas's channel (architecture, ADRs, investigations)
- `#kade` — Kade's channel (engineering, builds, tech debt)
- `#standup` — daily status updates from each role

### 4. Write Wrapper Scripts

Create scripts that any role can use from their Claude Code session:

**`slack-post.sh`**
```
Usage: slack-post.sh <channel> <message>
Example: slack-post.sh general "ADR-003 implementation complete. All tests green."
```

**`slack-read.sh`**
```
Usage: slack-read.sh <channel> [count]
Example: slack-read.sh general 10
```

Put these in `../messages/scripts/` and document usage.

The bot token should be read from an env var (`SLACK_BOT_TOKEN`) or a local config file (not committed to git).

### 5. Update CLAUDE.md Files

Add Slack instructions to all three CLAUDE.md files:
- On session start: check `#general` and your role channel for new messages
- When you produce something significant: post to `#general`
- For role-specific discussion: post to the relevant role channel
- The activity log (`../messages/activity.md`) remains the permanent record; Slack is for quick conversation

### 6. Test It

Post a test message from Kade's session to `#general`. Verify it shows up in the workspace.

## Constraints

- Free tier: 90 days message history, 10 app slots, unlimited channels/users
- Bot tokens and Web API work fully on free tier
- Legacy integrations are being deprecated — use the modern Slack App model
- Don't commit the bot token to git. Use env var or `.env` file in a gitignored location.

## Context

Jeff wants Slack for quick conversations where he can invite one or more roles. This mirrors his recent working style managing engineering teams. The briefs/activity log system stays for formal handoffs; Slack is for "hey, quick question" and real-time coordination.

Note: Jeff needs to create the workspace himself (requires a human to sign up at slack.com). Your job is everything after that — app creation, channels, scripts, documentation. If there's a way to automate workspace creation via CLI, go for it, but it likely requires browser interaction.

— Wren
