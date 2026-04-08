# Brief: Wire /chorus context into /werk

**From**: Wren (PM) → Silas (Architect)
**Date**: 2026-02-21
**Priority**: P2

## Context

Jeff invoked `/werk` and got the workflow state — clean, clear, beautiful dashboard. Then he asked: "Maybe /chorus is called by /werk so you also get any other memory context?"

Right now `/werk` shows workflow state only. `/chorus` shows team history (Slack, sessions, artifacts). They're separate views of the same system. Jeff wants one command that gives full situational awareness: workflows in flight + relevant team context.

## What Jeff Wants

When `/werk` runs, it should also pull relevant `/chorus` context — recent activity, related session history, anything that gives the workflow steps more meaning. Not a dump of everything — just what's relevant to the active workflows.

## Design Questions for You

1. **Scope**: Does `/werk` call the chorus index directly, or does it call `/chorus` as a sub-skill? The index query is probably cleaner.
2. **Relevance filtering**: How do we scope the chorus query to what matters? By card number? By role? By time window? Active workflow card tags (#115, #118) could be the join key.
3. **Output format**: Inline with the workflow list? Separate section? Progressive disclosure (show workflows, offer to drill into context)?
4. **Dashboard integration**: The HTML dashboard (that gorgeous value stream x roles grid) could also embed recent chorus context per workflow card.

## My Recommendation

Start with the CLI: when `workflow.sh list` or `workflow.sh status` runs, append a "Recent context" section that queries the chorus index for messages matching the workflow's card numbers. Keep the dashboard change for a second pass.

The join key is already there — workflows have card tags, chorus index has card references in messages. Wire them together.
