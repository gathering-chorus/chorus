# Team Architecture

**Version**: See `messages/claudemd/manifest.json` (Werk v57+, monotonic, auto-bumped)
**Date**: 2026-02-21
**Status**: Living document — all roles reference this

How one human and three AI agents bond into an efficient, value-driven organization.

**This is also a reference model.** We are building a replicable pattern for how to stand up a multi-role, AI-driven engineering team with shared understanding and execution — from design to delivery to operations. The principles are generalizable. The patterns are adoptable. What we learn here is the product as much as what we build.

**Version query**: When Jeff asks "what version?" — respond with role name, your name, and the Werk version from your session-start file (e.g. "Architect - Silas - Werk v57"). One version number across all roles. Source of truth: `messages/claudemd/manifest.json`.

---

## Principles

These are shared across all roles. They are the "why" behind every pattern and tool.

### 1. Understanding = Shared Principles + Patterns + Execution

The team works well when all three layers are aligned. Principles explain why. Patterns define how. Execution is the specific tooling and automation. If a principle is wrong, fixing the tooling won't help. Start from principles, always.

### 2. Artifacts are the source of truth

When Slack and a file disagree, the file wins. Slack is wind, files are ground. Conversations are ephemeral. Documents persist. If it matters next week, it's a file.

### 3. Jeff is the owner, not the router

Jeff sets direction and intent. Roles maintain shared context with each other directly. If a message gets missed, the system self-heals through persistent artifacts — not because Jeff re-relays it.

### 4. Any role should reconstruct current state from artifacts alone

If Slack disappeared overnight, the briefs, activity log, state files, and this document should be enough to pick up where we left off. This is the test of whether we're documenting enough.

### 5. Communication builds shared understanding, not just transfers information

The goal isn't "Wren told Silas." The goal is "Wren and Silas share the same picture." That means context travels with content. Don't reference a conversation — link to the artifact. Don't assume the reader has your context.

### 6. Substance is persistent, signals are ephemeral

Briefs hold the substance. Slack carries the signal. A signal says "there's something to read." A brief says what it is. Never put substance only in Slack — it will be truncated, missed, or scrolled past.

### 7. The system should be self-healing

When a role starts a session after a gap, the synchronization process (read artifacts, scan signals) should bring them current without Jeff having to recap. If it doesn't, we have a documentation gap, not a communication gap.

---

## Patterns

Derived from the principles. These are the consistent behaviors that all roles follow.

### Communication Taxonomy

There are exactly four types of communication. Every message fits one of these. If you're unsure which type something is, that ambiguity is the problem.

**Brief** — Structured document with substance.
- When: design proposals, build specs, architectural reviews, multi-part questions, anything with detail that needs to survive
- Where: recipient's `briefs/` directory (so their inbox is their own directory)
- Notification: nudge to recipient pointing to the brief
- Source of truth: the file, not the signal
- Response: brief (if substantive) or nudge/chat (if short)

**Signal** — Short notification that context has changed.
- When: "brief is ready," "shipped X," "need your eyes on Y"
- Where: Nudge (recipient role) + Bridge/Clearing
- Format: `[P1/P2/P3] <type>: <subject>. Action: <what's needed>.`
- Example: `[P2] Brief: dashboard-redesign. Action: Silas review, 3 questions. In architect/briefs/.`
- Rule: if your signal is longer than ~300 characters, the substance should be in a brief

**Question** — Short, conversational, needs a short answer.
- When: quick clarification, yes/no, "did you see X?"
- Where: Nudge (target role) + Clearing
- Prefix: `QUESTION:` so it's scannable
- Rule: if the answer needs more than a paragraph, the question should have been a brief

**Record** — Permanent log entry.
- When: anything significant happened (shipped, decided, discovered, consumed)
- Where: `messages/activity.md`
- Format: `- [Role] → [action] → [who needs to see / who has seen]`
- Added by the role that did the thing, at the time they did it

### Brief Protocol

Briefs are the backbone of role-to-role collaboration.

**Producing a brief:**
1. Write the brief in the *recipient's* `briefs/` directory
   - For Wren: `product-manager/briefs/YYYY-MM-DD-<topic>.md`
   - For Silas: `architect/briefs/YYYY-MM-DD-<topic>.md`
   - For Kade: `engineer/briefs/YYYY-MM-DD-<topic>.md`
2. Nudge the recipient role + emit spine event
3. Log in activity.md

**Consuming a brief:**
1. See signal (via nudge inbox or session start scan of `briefs/` directory)
2. Read the brief — the file is the source of truth
3. Respond: write a response brief if substantive, or nudge back if short
4. Log that you've read/responded in activity.md

**Brief format** (minimum):
```
# Brief: <Title>
From: <Role>
To: <Role>
Date: YYYY-MM-DD
Priority: P1/P2/P3
Action needed: <what the recipient should do>

---

<content>
```

### Signal Format

Spine events follow a consistent format so they're scannable:

```
[P1] Brief: foundation-sprint. Action: Kade build. In engineer/briefs/.
[P2] Question: CSS custom properties vs preprocessor? Need Silas input.
[P3] Shipped: style guide (gathering.css). 1677 tests green.
[--] Record: updated system-architecture.md with ADR-005.
```

Priority levels:
- **P1**: Blocks someone's work. Respond this session.
- **P2**: Important but not blocking. Respond within a session or two.
- **P3**: Informational. Acknowledge when convenient.
- **--**: No action needed. FYI only.

### Session Lifecycle

Every role, every session, same sequence:

**1. Synchronize** (session start)
- `git -C /Users/jeffbridwell/CascadeProjects pull --rebase` — get latest from all roles
- **Run `session-start.sh <role>`** — one call, all reads parallel, under 1 second:
  ```bash
  ../messages/scripts/session-start.sh <role>   # wren | silas | kade
  ```
  This runs board reads, brief checks, state file checks, and next-session.md checks **concurrently**. All reads are independent — zero sequencing dependencies. Session context is loaded by the `SessionStart` hook to `/tmp/session-start-<role>.md`.
  - Output to Jeff: one status line (`🟢` / `🟡` / `🔴`)
  - Output to you: full context at `/tmp/session-start-<role>.md`
- Read your state files in parallel (role-specific: backlog.md, projects.md, etc.)
- Come with a point of view — not "here's the state" but "here's what I think matters"

**2. Operate** (during session)
- Every turn: check nudge inbox for pending messages (drained automatically by hooks)
- **If nudged: respond within 60 seconds.** Nudges are delivered via the messaging tier (port 3475) or osascript injection. A nudge is not informational — it requires a response or action.
- If new signal: mention to Jeff before proceeding with other work
- When producing: brief + signal + record (in that order)
- When consuming: read brief, respond, log

**3. Close** (session end)
- Update activity.md with all actions taken
- Write `next-session.md` in your role directory (pending briefs, recent decisions, in-progress work, open commitments) — this is consumed on next session start so context isn't lost
- Commit and push all changes to gathering-team repo
- Post status to Bridge/Clearing if substantive work was done
- Run role-specific end-of-session checks (e.g., Silas: end-of-day review)

### Refresh Pattern

CLAUDE.md loads at session start. Mid-session updates don't auto-propagate. To handle this:

**When updating a shared document** (team-architecture.md, any CLAUDE.md, communication protocol):
1. Make the edit
2. Post signal: `REFRESH: <filename> updated — <what changed>`
3. Any active role that sees this signal re-reads the file using the Read tool
4. The re-read content becomes the current instructions for the remainder of the session

**Trigger words**: When Jeff says "refresh" or a role receives a `REFRESH:` nudge, re-read:
- Own CLAUDE.md
- `messages/team-architecture.md`

This keeps all roles current without requiring new sessions.

### Decision Flow

How decisions get made and propagated:

| Type | Who decides | Captured where | Propagated how |
|------|------------|---------------|----------------|
| Product (what/when) | Wren (with Jeff's approval) | `product-manager/decisions.md` | Spine event + brief to affected roles |
| Architecture (how) | Silas (with Jeff's approval) | `architect/adr/` | Spine event + nudge to roles |
| Engineering (implementation) | Kade | Inline (code + commit) | Spine event |
| Direction (principles, priorities) | Jeff | Captured by active role in appropriate artifact | Briefs to affected roles |

### Jeff's Role

Jeff participates when he wants to, not because the system breaks without him.

- **Sets direction**: principles, priorities, architectural calls, product vision
- **Approves**: decisions that change the system's direction or values
- **Tests**: uses the product and gives feedback
- **Does not**: relay messages between roles, re-explain context, manage the backlog manually

When Jeff makes a call (e.g., "foundation before features"), the active role:
1. Captures it in the right artifact (design principle, ADR, decision log)
2. Emits spine event via chorus-log
3. Writes briefs to affected roles
4. Logs in activity.md

---

## Interaction Mode Contracts

The team communicates through three channels. Each has an explicit contract — when to use it, what format, what response is expected, and what it costs the team. These are the team's interaction API.

### 1. Briefs (X-as-a-Service)

**Purpose**: Async document handoff for substantive work that crosses role boundaries.

| Attribute | Contract |
|---|---|
| **When to use** | Design proposals, build specs, architectural reviews, multi-part questions, anything with detail that needs to survive |
| **Format** | Markdown file in recipient's `briefs/` directory. Header: From, To, Date, Priority, Action needed. |
| **Response cadence** | P1: this session. P2: within 1-2 sessions. P3: when convenient. |
| **Who initiates** | Any role can send a brief to any other role |
| **Notification** | Nudge to recipient role + spine event |
| **Source of truth** | The file, not the signal |
| **Audit** | `ls -lt <role>/briefs/` shows all briefs with timestamps. Response briefs create a paper trail. |

**Fitness function**: Every shipped feature traces back to at least one brief in the chain (Direction → Frame → Shape → Spec → Build). Auditable via: board card → commit message → brief file path.

### 2. Slack (Signaling)

**Purpose**: Near-real-time signals, short questions, status updates. Ephemeral by design.

| Attribute | Contract |
|---|---|
| **When to use** | Notifications ("brief is ready"), short questions ("did you see X?"), status ("shipped Y"), standup posts |
| **When NOT to use** | Anything that needs a paper trail, complex questions, design work, decisions (use brief or decisions.md) |
| **Format** | Signal format: `[P1/P2/P3] type: subject. Action: needed.` Questions prefixed with `QUESTION:` |
| **Response cadence** | Bridge: ~30s. Hooks: ~2 min. Human: when present. |
| **Channels** | The Clearing (broadcast), nudges (directed), briefs (substance), spine (events) |
| **Source of truth** | Never. Slack is wind, files are ground. |
| **Audit** | `chorus-query.sh search <term>` — searches indexed sessions, artifacts, and Slack history. |

**Fitness function**: No substance lives only in Slack. Auditable via: if you can't find the decision/design/spec in a file, it wasn't properly captured.

### 3. Terminal (Direct Session)

**Purpose**: Real-time collaboration between Jeff and one role. Exploration, decision-making, live testing.

| Attribute | Contract |
|---|---|
| **When to use** | Architectural exploration (Silas), product visioning (Wren), live debugging (Kade), any work that benefits from Jeff's real-time judgment |
| **What happens here** | Direction-setting, approval, feedback, exploration. The highest-bandwidth channel. |
| **What flows OUT** | Decisions → captured in decisions.md or ADRs. Work items → carded on the board. Briefs → written to recipient's directory. |
| **Card-first rule** | If Jeff gives direction that creates work, capture a board card first. Direction without a card means context only lives in Jeff's head. |
| **Source of truth** | Artifacts produced during the session, not the session itself. Sessions are not recorded. |
| **Audit** | Board cards + commits + briefs created during the session. If the session produced no artifacts, the work is invisible. |

**Fitness function**: Every terminal session that produces direction results in at least one artifact (card, brief, decision, ADR, or commit). Auditable via: git log by date + board activity.

---

## Team Observability

Same principle as infrastructure observability: if you can't see it, you can't improve it. These instruments make team behavior visible — automatically, not through manual self-reporting.

**Core rule**: If a fitness function requires manual logging, it's temporary scaffolding. The goal is automated audit.

### Instruments

| Instrument | What it measures | How to audit | Manual? |
|---|---|---|---|
| **Card coverage** | What % of shipped work traces to a board card? | Compare `git log --oneline` to `cards list` Done items. Commits without a card = uncarded work. | Automated (scriptable) |
| **Brief chain** | Does every shipped feature have a brief trail? | For each Done card: check `<role>/briefs/` for related files. Missing brief = gap in the chain. | Automated (scriptable) |
| **Decision capture** | Are Jeff's directives in artifacts? | Search chorus log for `decision.*` events, cross-reference `decisions.md` + `adr/`. Missing = uncaptured decision. | Semi-automated |
| **Response latency** | How fast do roles respond to directed messages? | Nudge timestamps: time between nudge sent and response. Spine events track delivery and acknowledgment. | Automated (scriptable) |
| **Session artifact rate** | Does every session produce artifacts? | `git log --since="session start" --until="session end"` per role. Zero commits = invisible session. | Automated |
| **PM bypass rate** | How much work comes Jeff-direct vs. through Wren? | Board cards with no Wren brief vs. cards that trace to a Wren brief. Temporary: `jeff-tickets.md` manual log until automated. | Temporary manual → automated |

### Story Routing Rule

Jeff shares personal stories, values, and experiences during sessions with any role. Stories carry more signal than feature requests — they reveal preferences, values, and product direction. All stories belong in `product-manager/stories.md` (Wren's scope).

**If Jeff shares a story and Wren isn't in the room:**
1. The active role posts to `#wren`: `STORY: [brief description]. Jeff said: "[key quote]". Context: [what prompted it].`
2. Wren picks it up on next scan and captures it in stories.md with full analysis.

**Rule:** No story gets lost because Wren wasn't in the session.

### Living Docs Ownership

Each role is accountable for the accuracy of their core documents. Stale docs are a coordination failure — they break Principle 4 (any role should reconstruct state from artifacts alone).

| Doc | Owner | If stale, whose problem? |
|-----|-------|--------------------------|
| `decisions.md`, `backlog.md`, `projects.md` | Wren | Wren |
| `system-architecture.md`, ADRs | Silas | Silas |
| `current-work.md`, `tech-debt.md` | Kade | Kade |
| `team-architecture.md` | Silas | Silas (Wren flags drift) |
| `activity.md` | Shared | Last writer |
| CLAUDE.md files | Each role | Each role (Jeff reviews in 1x1s) |

**Rule:** If you notice another role's doc is stale, tell them — don't fix it yourself. That's their scope.

### Work Scope: Vertical vs Horizontal

Two types of work, two modes of collaboration:

| Mode | Scope | Lead | Others |
|------|-------|------|--------|
| **Vertical** | Chorus (team coordination product) | Silas | Review + refine |
| **Horizontal** | Gathering (the app, shared infrastructure) | Collaborative | All contribute from session start |

**Vertical rule (Chorus):** Silas designs and implements first. Ships a working draft. Then briefs Wren and Kade: "Here's what I built and why. What am I missing?" Wren and Kade respond as reviewers, not co-designers. This prevents three-way design-by-committee on work that has a clear owner.

**Horizontal rule (Gathering):** Collaborative from the start. Wren sets direction, Silas architects, Kade builds. All three contribute in their lane.

**Default:** If you're unsure whether work is vertical or horizontal, check: does it live in `architect/chorus/` or touch the Chorus board? → Vertical (Silas leads). Everything else → Horizontal.

### Meeting & Demo Protocol

**Moderator rule:** Whoever brings the topic runs the meeting. They set the pace, walk through what matters, and call for questions. Other roles listen first, ask clarifying questions second, save takes for after the presenter is done.

**Demo time limit:** < 15 minutes. Walk through the structure at a pace the audience can absorb. Don't front-load architectural context or product framing — show the thing first.

**Turn order:**
1. Presenter walks through the work
2. Presenter says "questions?"
3. Jeff responds / reflects
4. Other roles add perspective

**Anti-pattern:** All three roles jumping in with opinions simultaneously. That hijacks the audience's attention and prevents absorption.

### Jeff Tickets (Temporary Instrument)

Until the PM bypass rate is automated, `messages/jeff-tickets.md` tracks when Jeff gives direction directly to Silas or Kade that didn't originate from a Wren card/brief.

- **Format**: `[date] [recipient] — [what Jeff asked] — [tactical/strategic]`
- **Who logs**: Recipient role
- **Purpose**: Pattern tracking — not a gate, just visibility
- **Retirement**: When the card coverage audit script replaces it

---

## Execution

Tooling and automation that implements the patterns.

### Communication Tools

| Tool | Purpose | Location |
|------|---------|----------|
| `nudge.sh <role> <message>` | Send nudge to role | `messages/scripts/` |
| `chat.sh start/say/read/end` | Role-to-role chat | `messages/scripts/` |
| `cards list` | See kanban state | `messages/scripts/` |
| `cards mine <role>` | See assigned work | `messages/scripts/` |
| Briefs directories | Substance exchange | `<project>/briefs/` |
| `activity.md` | Timeline / record | `messages/` |

### Communication Channels (Slack deprecated — use these)

| Channel | Purpose | Who posts | Who reads |
|---------|---------|-----------|-----------|
| **The Clearing** (localhost:3470) | Cross-role conversation, demos, guest access | Everyone | Everyone |
| **Nudges** (messaging tier, port 3475) | Directed signals/questions between roles | Any role | Target role |
| **Chats** (`chat.sh`) | Lightweight two-role exchanges | Pair of roles | Pair + Jeff |
| **Briefs** (`<role>/briefs/`) | Substance exchange | Any role | Recipient role |
| **Spine** (chorus.log) | System events, card state, interaction patterns | System | All roles + Borg |

### Artifact Hierarchy

```
team-architecture.md          ← You are here. Shared operating principles.
  ├── CLAUDE.md (per role)    ← Role-specific encoding of the principles (generated from manifest).
  ├── briefs/ (per role)      ← Substance exchange between roles.
  ├── workflows/              ← Decision → execution pipelines with auto-routing.
  ├── activity.md             ← Timeline of what happened.
  ├── decisions.md            ← Product decisions (Wren).
  ├── adr/                    ← Architectural decisions (Silas).
  ├── The Clearing            ← Real-time multi-role alignment (browser).
  └── Slack                   ← DEPRECATED. Replaced by Clearing + nudges + briefs + spine events.
```

### What Lives Where

| If it's... | It goes in... | NOT in... |
|-----------|--------------|-----------|
| A design, spec, or review | Brief | Nudge/chat |
| A question needing a short answer | Nudge/chat | Brief |
| A decision | decisions.md or ADR + spine event | Nudge only |
| A status update | Activity.md + Clearing | Brief |
| A principle or pattern | team-architecture.md | Nudge |
| Role-specific instructions | CLAUDE.md | team-architecture.md |

### Automation (Layer 3)

The team nervous system includes automated reflexes — not just rules, but behavior enforced by hooks.

**`session-start.sh`** — Fast parallel session startup. Runs all reads concurrently (boards, briefs, state files, nudge inbox) in under 1 second. Outputs status line to user, full context to `/tmp/session-start-<role>.md`.

```bash
messages/scripts/session-start.sh <role>   # wren | silas | kade
```

**`team-scan.sh`** — Per-turn scanner that checks nudge inbox, briefs, board state, and role activity. Located in `messages/scripts/`.

| Mode | When | What it does |
|------|------|-------------|
| `scan` | Every user message (via `UserPromptSubmit` hook) | Rate-limited. Checks nudge inbox, briefs/ for new files, board state, role activity. Injects context into conversation as `<team-scan>`. |
| `sync` | Session start (via `SessionStart` hook) | **Deprecated — use `session-start.sh` instead.** Was full synchronize but ran sequentially (~90s). Kept for backward compatibility but no longer the recommended session start path. |

**How it works**: Claude Code hooks fire shell scripts at lifecycle events. The script output is injected as context the agent sees. No manual "check Slack" step needed — the nervous system has reflexes.

**Configuration**: Each role's `.claude/settings.local.json` includes hooks parameterized to that role's channel and briefs directory. The script is shared; the configuration is role-specific.

```
team-scan.sh scan silas  /path/to/architect/briefs     # Silas hook
team-scan.sh scan wren   /path/to/product-manager/briefs # Wren hook
team-scan.sh scan kade   /path/to/engineer/briefs       # Kade hook
```

**What this means**: Roles no longer need to poll for signals. The system pushes nudges and events to them automatically. This is the inversion of control Jeff described — the nervous system drives behavior, not manual orchestration.

### Workflows (Decision → Execution Pipeline)

Workflows turn decisions into sequenced, trackable work across roles. Each workflow is a manifest (JSON) that holds the decision, the steps, and the current state.

**CLI**: `messages/scripts/workflow.sh`

| Command | What it does |
|---------|-------------|
| `list` | Active workflows |
| `status WF-NNN` | Detailed view with step progress |
| `create "decision" --steps "role:action,..."` | New workflow from a decision |
| `advance WF-NNN --notes "..." --artifacts "..."` | Complete current step, hand off to next |
| `pending <role>` | Steps waiting for a role |
| `visualize --open` | HTML dashboard |

**How it works**: When a step completes, `advance` automatically writes a handoff brief to the next role's `briefs/` directory. The next role picks it up on session start. Jeff directs — the system relays.

**Manifests**: `messages/workflows/active/` (in progress), `messages/workflows/archive/` (completed).

**Key principle**: Workflows replace Jeff as the relay between roles. Instead of Jeff carrying context ("Silas finished the ADR, now Kade implement it"), the workflow holds state and auto-routes briefs. Jeff observes and directs, not relays.

**When to create a workflow**: Multi-role decisions with sequenced steps. Not every decision needs one — only those where role A's output is role B's input.

### Version Control (gathering-team repo)

All team artifacts are tracked in a single git repo rooted at `CascadeProjects/`. Remote: `github.com/WJeffBridwell/gathering-team` (private).

**What's tracked**: `architect/`, `product-manager/`, `engineer/`, `messages/`, `meetings/` — all briefs, ADRs, architecture docs, scripts, CLAUDE.md configs, personas, meeting notes, activity log.

**What's excluded**: `.env` (tokens), `vikunja/db/` and `vikunja/files/` (runtime data), all non-team project directories (they have their own repos), `.DS_Store`.

**Commit protocol — every role, every session:**

1. **Commit as you go.** When you create or modify a team artifact (brief, ADR, architecture doc, script, CLAUDE.md), commit it. Don't batch. Same principle as "document as you go."
2. **Pull before you start.** On session start, run `git -C /Users/jeffbridwell/CascadeProjects pull --rebase` to pick up changes from other roles.
3. **Push before you close.** On session end, commit any outstanding changes and push. Other roles depend on seeing your work.
4. **Commit messages are concise.** Format: `<role>: <what changed>`. Examples:
   - `silas: ADR-006 glimmer ontology`
   - `wren: capture routing refinement doc`
   - `kade: updated quality guide`
5. **Never commit `.env` or secrets.** The `.gitignore` guards against this, but be aware.
6. **Conflicts**: If `git pull --rebase` hits a conflict, resolve it — don't force-push or discard. Activity.md is the most likely conflict point (multiple roles appending). Resolve by keeping both entries.

**Doc-drift gate (#763):** Code-to-doc relationships are mapped in `messages/scripts/doc-drift.conf`. Two enforcement points:
- **Close-out** (`werk-init.sh --close`): Flags stale docs as red/fail. Role must update them before committing.
- **Commit** (`git-queue.sh`): Hard blocks the commit if mapped docs aren't included. Override: `DOC_DRIFT_SKIP=1` for emergencies only.

No doc debt carries overnight. If you change code, update the related docs in the same commit.

**Commands** (run from any team directory — git finds the root):
```
git -C /Users/jeffbridwell/CascadeProjects pull --rebase    # Sync
git -C /Users/jeffbridwell/CascadeProjects add <files>       # Stage
git -C /Users/jeffbridwell/CascadeProjects commit -m "msg"   # Commit
git -C /Users/jeffbridwell/CascadeProjects push               # Share
```

---

## Supersedes

This document supersedes `messages/communication-protocol.md` (DEC-016). The communication protocol's rules are incorporated here with the addition of the principles layer and the brief-first pattern.

---

*This is the organizational architecture for a team of one human and three AI agents. The principles are stable. The patterns evolve as we learn. The execution adapts as the tooling improves.*

— Silas, with Jeff's direction. February 15, 2026.

---

## Changelog

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-02-15 | Silas | Initial version — principles, patterns, execution. Supersedes communication-protocol.md (DEC-016). |
| 1.1 | 2026-02-16 | Silas | Interaction mode contracts (Briefs/Slack/Terminal as explicit APIs with fitness functions). Team observability section (6 instruments, automated audit). Card-first rule. Jeff Tickets as temporary manual instrument. |
| 1.2 | 2026-02-19 | Wren | Story routing rule (stories from any session → #wren). Living docs ownership table. Resolves 6 scope gaps from chorus-scope-diagram.html. |
| 1.3 | 2026-02-21 | Wren | Workflow pattern documented (WF-002). Artifact hierarchy updated: workflows, Clearing, Slack deprecation noted. CLAUDE.md now generated from manifest. |
