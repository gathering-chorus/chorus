# Brief: Core Engineering Docs & Team Policies

**From**: Wren (PM)
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: High — this changes how we work

## Context

Jeff, Silas, and I are aligning on how the team operates. Three new decisions (DEC-013, DEC-014, DEC-015) that affect everyone. Summary:

1. **Six core docs only** — everything else is transient or reference
2. **Model-driven workflow** — changes flow from conceptual model → architecture → implementation
3. **Perennials vs annuals** — perennial standards for durable artifacts, annual speed for features/experiments

## Your Core Docs

Same ask I gave Silas: identify which of your files are living documents vs. reference. My read:

- **Living (refine)**: `current-work.md`, `tech-debt.md`
- **Reference (snapshot)**: `quality-guide.md`, `acl-audit-results.md`

These aren't in the team's top 6 — they're your working docs. But they should be maintained, not proliferating. `current-work.md` is your scratch pad for what's in flight. `tech-debt.md` tracks what's deferred. Everything else serves those.

Your call on what's right. If you have docs I'm not seeing that are important, flag them.

## Team Policies to Embed

These are emerging policies from today's session. Please build them into how you operate:

### 1. Pace
Jeff said: "You all are too fast for me." He trusts what we produce but doesn't always absorb it. Before shipping something significant, make sure Jeff (or Wren as proxy) has had a chance to understand what's changing and why. Don't optimize for throughput — optimize for Jeff's comprehension.

### 2. Perennials vs Annuals
- **Perennial work** (ontology changes, ACL model, core middleware): Apply rigor. Tests, ADR, review cycle.
- **Annual work** (Twilio webhook, Vikunja setup, wrapper scripts): Move fast, keep it simple, compost if it doesn't work.
- The Vikunja setup brief you have? That's annual work. Get it running, don't over-engineer it.

### 3. Model-Driven Flow
When Jeff or Wren proposes a new concept (e.g., "captures should be first-class"), it lands in the conceptual model first, gets Silas's architectural validation, then becomes your build brief. You shouldn't be building things that aren't in the model yet — or if you are, flag that the model needs updating.

Exception: Infrastructure/tooling work (CI, scripts, Docker) doesn't need to go through the model. Use judgment.

### 4. Digital Inheritance
New product framing: Gathering is a legacy artifact — a digital inheritance from Jeff. This means the Feeling layer (personal annotations, meaning, context) is the highest-value feature long-term. When you're building anything that touches personal metadata, treat it with extra care. Depth over breadth.

### 5. Communication
- 1:1 with Jeff → Claude Code session
- Multi-role → Slack (#all-gathering or role channels)
- Check Slack on session start
- Activity log (`../messages/activity.md`) remains the permanent record

## What's on Your Plate

In priority order:
1. **Vikunja setup** (brief already sent — `2026-02-14-kanban-tool-setup.md`) — annual work, move fast
2. **Exploratory UI testing support** — Jeff wants to test UI flows after your changes
3. **Access control permutation matrix** — map 90 permutations against E2E coverage, find gaps

— Wren
