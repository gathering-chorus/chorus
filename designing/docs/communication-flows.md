# Communication Flows — Sequence Diagrams

**Author:** Silas (Architect)
**Date:** 2026-02-19

How Jeff communicates with each team member, what the polling/reply loops look like, and where the gaps are.

---

## 1. Direct Session (Synchronous)

Jeff opens a Claude Code terminal in a role's directory. Real-time, turn-by-turn.

```mermaid
sequenceDiagram
    participant Jeff
    participant Hook as UserPromptSubmit Hook
    participant Scan as team-scan.sh (scan mode)
    participant Slack as Slack API
    participant Role as Role (Wren/Silas/Kade)

    Jeff->>Hook: Types prompt
    activate Hook
    Hook->>Scan: team-scan.sh scan <role> <briefs-dir>
    activate Scan
    Note over Scan: Rate-limited: once per 2 min
    alt Scan interval elapsed
        Scan->>Slack: slack-read.sh #role (3 msgs)
        Slack-->>Scan: Recent messages
        Scan->>Slack: slack-read.sh #all-gathering (5 msgs)
        Slack-->>Scan: Recent messages
        Scan-->>Hook: <team-scan> context (if anything new)
    else Scan interval not elapsed
        Scan-->>Hook: (silent — no output)
    end
    deactivate Scan
    Hook-->>Jeff: Context injected into prompt
    deactivate Hook
    Jeff->>Role: Prompt + injected context
    activate Role
    Role-->>Jeff: Response (immediate)
    Note over Role: May also write briefs, post to Slack
    deactivate Role
```

**Latency:** Seconds (LLM response time)
**Reply mechanism:** Direct — Role responds in terminal
**Jeff required:** Yes — every interaction needs Jeff to type

---

## 2. Session Start (Synchronous, One-Time)

When Jeff opens a new session for a role.

```mermaid
sequenceDiagram
    participant Jeff
    participant Start as SessionStart Hook
    participant Sync as team-scan.sh (sync mode)
    participant Audit as chorus-audit.sh
    participant Slack as Slack API
    participant FS as Filesystem
    participant Role as Role (Wren/Silas/Kade)

    Jeff->>Start: Opens new session
    activate Start
    par Sync scan
        Start->>Sync: team-scan.sh sync <role> <briefs-dir>
        activate Sync
        Sync->>Slack: slack-read.sh #role (10 msgs)
        Slack-->>Sync: Messages
        Sync->>Slack: slack-read.sh #all-gathering (20 msgs)
        Slack-->>Sync: Messages
        Sync->>FS: Check briefs/ for new files (mtime vs marker)
        FS-->>Sync: New brief list
        Sync->>FS: Read last 20 lines of activity.md
        FS-->>Sync: Recent activity
        Sync-->>Start: Full sync context
        deactivate Sync
    and Audit
        Start->>Audit: chorus-audit.sh start <role>
        activate Audit
        Note over Audit: Checks: commits pushed, uncommitted files,<br/>activity.md freshness, standup, cost, disk health
        Audit-->>Start: Gate results (pass/warn/fail)
        deactivate Audit
    end
    Start-->>Jeff: Sync context + audit results injected
    deactivate Start
    Jeff->>Role: First prompt with full context
    Role-->>Jeff: Session begins
```

**Latency:** 15-30 seconds (Slack API calls + audit checks)
**This is the only time a role "catches up"** on what happened while it was idle

---

## 3. Slack Bridge (Asynchronous — No Jeff Required)

The bridge responds to Slack messages when Jeff is away.

```mermaid
sequenceDiagram
    participant Anyone as Jeff / Other Role
    participant Slack as Slack Channel
    participant Bridge as Slack Bridge (Docker)
    participant Claude as Claude API (Sonnet)

    loop Every 30 seconds
        Bridge->>Slack: Poll channels (#silas, #wren, #kade, #all-gathering)
        Slack-->>Bridge: New messages (since last-seen timestamp)

        alt New message found (not from bridge)
            Note over Bridge: Route: #silas → Silas, #all-gathering → mentioned roles
            Bridge->>Bridge: Rate limit check (15/hr per role, 30/hr global)

            alt Within rate limit
                Bridge->>Bridge: Assemble context (CLAUDE.md, briefs list, activity.md)
                Bridge->>Claude: API call (Sonnet, 1024 tokens, temp 0.7)
                Claude-->>Bridge: Response text
                Bridge->>Slack: Post response + "··bridge" marker
                Note over Slack: Bridge responses are read-only opinions.<br/>Cannot write files, run commands, or commit.
            else Rate limited
                Note over Bridge: Skip — too many responses this hour
            end
        else No new messages (or all from bridge)
            Note over Bridge: Sleep until next poll
        end
    end

    Note over Anyone: Jeff sees bridge responses next time he checks Slack
```

**Latency:** ~30-40 seconds (poll interval + API call)
**Reply mechanism:** Yes — bridge posts to Slack channel
**Jeff required:** No — runs autonomously
**Limitation:** Bridge can only read context, cannot write files or take actions

---

## 4. Role → Role via Brief + Signal (Asynchronous)

How roles communicate substantive content to each other.

```mermaid
sequenceDiagram
    participant RoleA as Role A (e.g., Wren)
    participant FS as Filesystem
    participant Slack as Slack API
    participant Bridge as Slack Bridge
    participant RoleB as Role B (e.g., Silas)

    Note over RoleA: Produces a brief with architectural question
    RoleA->>FS: Write brief to role-b/briefs/2026-02-19-topic.md
    RoleA->>Slack: slack-post.sh #silas "Brief in your inbox — ..."

    alt Bridge is running
        Note over Bridge: Next poll cycle (~30s)
        Bridge->>Slack: Polls #silas
        Slack-->>Bridge: Sees Wren's signal
        Bridge->>Slack: Posts acknowledgment + preliminary thoughts
        Note over Slack: "··bridge" marker appended
    end

    Note over RoleB: === Time passes (minutes to hours) ===

    alt Jeff opens Silas session
        RoleB->>Slack: SessionStart hook reads #silas (10 msgs)
        Slack-->>RoleB: Sees Wren's signal + bridge acknowledgment
        RoleB->>FS: SessionStart hook checks briefs/ mtime
        FS-->>RoleB: Detects new brief
        Note over RoleB: "New brief: 2026-02-19-topic.md"
        RoleB->>FS: Reads the full brief
        RoleB->>FS: Writes response brief to role-a/briefs/
        RoleB->>Slack: Posts signal to #wren
    end
```

**Latency:** Minutes to hours (depends on when Jeff opens recipient's session)
**Bridge shortens perceived latency** — the bridge acknowledges within ~30s, but can't take action
**The brief is the substance, Slack is just the signal**

---

## 5. All Paths at a Glance

```mermaid
graph TD
    Jeff[Jeff]

    subgraph "Synchronous (Jeff present)"
        S1[Terminal Session<br/>Seconds latency]
        S2[Session Start Sync<br/>15-30s one-time]
        S3[Per-Prompt Scan<br/>Every 2 min]
    end

    subgraph "Asynchronous (Jeff optional)"
        A1[Slack Bridge<br/>~30s polling loop]
        A2[Brief + Signal<br/>Minutes to hours]
        A3[Slack Scripts<br/>One-shot, no reply]
    end

    subgraph Roles
        Wren[Wren]
        Silas[Silas]
        Kade[Kade]
    end

    Jeff -->|types prompt| S1
    S1 --> Wren & Silas & Kade
    Jeff -->|opens session| S2
    S2 --> Wren & Silas & Kade
    S1 -.->|triggers| S3

    Jeff -->|posts to Slack| A1
    A1 -->|bridge responds| Wren & Silas & Kade

    Wren -->|writes brief| A2
    A2 --> Silas & Kade
    Silas -->|writes brief| A2
    Kade -->|writes brief| A2

    Jeff -->|slack-post.sh| A3
```

---

## 6. Group Conversation via Bridge (@team pattern)

Jeff posts `@team` in `#all-gathering`. The bridge fans out to all three roles in a single API call, each responds in sequence.

```mermaid
sequenceDiagram
    participant Jeff
    participant Slack as #all-gathering
    participant Bridge as Slack Bridge
    participant Claude as Claude API (Sonnet)

    Jeff->>Slack: "@team — can we convene and share status?"
    Note over Bridge: Next poll (~30s)
    Bridge->>Slack: Polls #all-gathering
    Slack-->>Bridge: Sees @team message

    Bridge->>Claude: API call with all 3 role contexts
    Claude-->>Bridge: Wren response
    Bridge->>Slack: Posts Wren's response + "··bridge:wren"

    Claude-->>Bridge: Silas response
    Bridge->>Slack: Posts Silas's response + "··bridge:silas"

    Claude-->>Bridge: Kade response
    Bridge->>Slack: Posts Kade's response + "··bridge:kade"

    Note over Slack: All 3 responses appear within ~30s
    Note over Jeff: Jeff reads all 3, decides next move
```

### Known Problems with Group Conversations

1. **All roles respond simultaneously** — no moderator, no turn-taking. Jeff gets 3 walls of text at once.
2. **Roles talk past each other** — each role responds to Jeff's prompt, not to what the other roles said. No actual coordination happening.
3. **No pacing for absorption** — Jeff can't reflect on one response before the next appears.
4. **Echo loops** — bridge sometimes feeds a role's own message back as input, creating confusion.
5. **Conversation commitments don't persist** — roles promise to do things in Slack, but those commitments are lost when the session ends. Next session, same role re-discusses instead of showing progress.

### What's Missing: The Feedback Loop

```mermaid
sequenceDiagram
    participant Jeff
    participant Slack as Slack
    participant Bridge as Bridge
    participant Role as Role Session (Direct)
    participant Memory as Role Memory Files

    Jeff->>Slack: "@team do X"
    Bridge->>Slack: Role commits "I'll do X"
    Note over Memory: ❌ Commitment NOT written to memory

    Note over Jeff: === Hours/days pass ===

    Jeff->>Role: Opens direct session
    Role->>Memory: Reads memory files on session start
    Note over Role: ❌ No record of Slack commitment
    Role->>Jeff: Discusses X as if new
    Note over Jeff: "Didn't we already agree on this?"
```

**The fix (proposed):** Bridge writes commitments to role memory files (`<role>/slack-context.md`). On session start, role reads this alongside other state files. Commitment → memory → execution → progress report.

---

## 7. Demo / Walkthrough Protocol (NEW — 2026-02-20)

When a role demos work to Jeff via `@team` or direct session:

```mermaid
sequenceDiagram
    participant Presenter as Presenter (e.g., Kade)
    participant Jeff
    participant Others as Other Roles

    Presenter->>Jeff: Walks through the work (< 15 min)
    Note over Others: SILENT — listening only

    Presenter->>Jeff: "Questions?"

    Jeff->>Presenter: Reflects, asks questions
    Presenter->>Jeff: Answers

    Jeff->>Others: "Thoughts?"
    Others->>Jeff: Add perspective (brief, not walls of text)
```

**Rules:**
- Presenter sets the pace — not the other roles
- Other roles wait for "questions?" before speaking
- Demo < 15 minutes
- If a role sees a concern, note it for after — don't interrupt the flow

---

## Current Gaps (Updated 2026-02-20)

| # | Gap | Impact | Status |
|---|-----|--------|--------|
| G1 | **Idle sessions are deaf** | Role misses Slack messages until next prompt | Partially mitigated by team-scan hook (every 2 min) |
| G2 | **Bridge can't act** | Acknowledges but can't write files, move cards, commit | By design. Future: trusted action whitelist |
| G3 | **Role-to-role latency** | Brief sits until Jeff opens recipient's session | Bridge gives preliminary response; full processing waits |
| G4 | **Slack commitments don't persist** | Roles promise things in Slack, forget by next session | **Active gap.** Bridge auto-generates commitment briefs but creates 16+ per conversation — too noisy. Needs: bridge writes to `slack-context.md` per role, not individual brief files |
| G5 | **Group conversations lack moderation** | All 3 roles respond simultaneously, Jeff gets 3 walls of text | **New rule (2026-02-20):** Moderator protocol + demo time limit added to team-architecture.md |
| G6 | **Bridge echo loops** | Role's own message fed back as input | Known bug — Wren investigating |
| G7 | **No guaranteed brief delivery** | Brief written but Slack signal fails → recipient unaware | chorus-audit.sh checks briefs/ mtime on session start (partially fixed) |
| G8 | **Conversation commitment brief flood** | Bridge generates 16+ commitment briefs per conversation | Needs tuning: deduplicate, batch per conversation, or replace with append-to-file |
