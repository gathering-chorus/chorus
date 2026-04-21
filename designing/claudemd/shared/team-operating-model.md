## Team Operating Model

Full model: `../../../team-architecture.md`. Session lifecycle: **Synchronize** (SessionStart hook injects context into your first turn; read state files as needed) → **Operate** (brief + signal + record) → **Close** (update activity.md, commit).

**Close-out triggers** (don't wait for Jeff): "eod", "wrapping up", "done for today", past 5pm and winding down, or previous session missed close-out.

**Refresh**: When Jeff says "refresh", re-read this file and team-architecture.md.

**Brief routing**: Write briefs TO the recipient's directory, not your own:
{{BRIEF_ROUTING}}

## Exchanging Work Between Roles

Briefs are the primary mechanism. Write to the recipient's `briefs/` directory, not your own. Include: question/request, context, constraints, response needed. Log handoffs in `../../../activity.md`. Every handoff should be traceable.

## Team Activity Log

Shared audit trail at `../../../activity.md`. All roles read and append. Log when you produce or consume something (brief, decision, review). Format: `- [Role] → [action] → [who needs to see]`. Scan for new entries on session start.

## Multi-Role Discussions

Default: **briefs and responses** — not meetings. Jeff should not be the carrier between roles. Roles exchange work directly.

### The Clearing (`/clearing`)

For real-time multi-role alignment: `/clearing` starts a browser-based group chat with Jeff and all three AI roles. Mark decisions with `DECISION:` prefix — auto-captured. Transcripts indexed into Chorus. Use when async briefs are too slow.
