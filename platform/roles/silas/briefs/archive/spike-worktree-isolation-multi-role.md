# Spike Brief: Worktree Isolation for Multi-Role Architecture

From: Silas (Architect)
To: Jeff, Wren, Kade
Date: 2026-02-22
Priority: P2
Action needed: Read and discuss. No implementation proposed yet.

---

## Summary

Claude Code now offers three parallel-work mechanisms: **worktree sessions** (`--worktree`), **subagent worktree isolation** (`isolation: worktree`), and **agent teams** (experimental). This spike assesses how each maps to our 3-role model (Silas/Wren/Kade) and whether any of them could replace or augment the current architecture where Jeff manually switches between three separate Claude Code sessions.

**Bottom line: Agent teams are the closest fit conceptually, but the feature is experimental and its coordination model is fundamentally different from ours. Worktree isolation solves a problem we mostly do not have. Recommendation: wait.**

---

## 1. How Worktree Isolation Works

### CLI-level worktrees (`--worktree`)

- `claude --worktree feature-auth` creates a git worktree at `<repo>/.claude/worktrees/feature-auth/` with a new branch `worktree-feature-auth` based on the default remote branch.
- Each worktree is a full, independent working directory with its own files and branch, sharing the same repository history.
- On exit: if no changes were made, the worktree and branch are auto-removed. If changes exist, the user is prompted to keep or remove.
- The `--tmux` flag can create split panes (iTerm2 or tmux) so multiple sessions are visible simultaneously.
- Sessions in worktrees load the same CLAUDE.md and project context as normal sessions.

### Subagent worktree isolation (`isolation: worktree`)

- Custom subagents can specify `isolation: worktree` in their frontmatter.
- The subagent gets a temporary git worktree, works in isolation, and the worktree is auto-cleaned if no changes were made.
- Subagents run within a single parent session. They cannot spawn other subagents.
- Results return to the parent session's context window.
- Subagents do NOT inherit the parent's conversation history -- they get only their own system prompt plus basic environment details.

### Agent teams (experimental, `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`)

- A lead session spawns teammate sessions, each as a fully independent Claude Code instance.
- Teammates coordinate through a shared task list and a mailbox messaging system.
- Each teammate has its own context window (not shared with the lead or other teammates).
- Teammates can message each other directly and claim/complete tasks from the shared list.
- The lead creates tasks, assigns work, and synthesizes results.
- All teammates start with the lead's permission settings.
- Teammates read the same CLAUDE.md from their working directory.
- Known limitations: no session resumption for in-process teammates, one team per session, no nested teams, lead is fixed, permissions set at spawn time.

### Branch and Merge Behavior

**There is no automatic merge.** None of the three mechanisms handle merge convergence automatically:

- CLI worktrees create separate branches. The user or a coordinating session must merge them manually via `git merge` or PR.
- Subagent worktrees are temporary. If the subagent commits, those commits live on the worktree branch until explicitly merged.
- Agent teams have no built-in merge strategy. Anthropic's own C compiler experiment used a shared git repo with push/pull and relied on git's native conflict resolution -- agents would push commits and let conflicts surface naturally.

---

## 2. Mapping to Our 3-Role Model

### What we have now

| Aspect | Current model |
|--------|--------------|
| Sessions | 3 separate Claude Code sessions, one per role. Jeff switches manually. |
| Context | Each role loads its own CLAUDE.md (generated from shared fragments). Role-specific state files, briefs, and memory. |
| Coordination | Briefs (markdown files in recipient's `briefs/` dir), Slack messages, workflow engine (`workflow.sh`), activity.md log. |
| Git | Single branch (main). Each role commits only to its own directory (`architect/`, `engineer/`, `product-manager/`). No branching strategy. |
| Shared memory | Chorus context service (SQLite FTS5 index of Slack, sessions, artifacts). |
| Handoffs | workflow.sh auto-generates handoff briefs. Jeff directs but does not relay. |

### Option A: Worktree sessions (one per role)

**Mapping**: Jeff opens 3 terminal sessions with `claude --worktree silas`, `claude --worktree kade`, `claude --worktree wren`. Each gets its own worktree branch.

**What works**:
- File isolation -- roles cannot accidentally overwrite each other's work.
- Each worktree loads the correct CLAUDE.md.
- `--tmux` gives split-pane visibility of all three roles.

**What breaks**:
- Our roles do not need file isolation. They already commit only to their own directories. The directory-ownership model is already sufficient separation.
- Worktree branches diverge from main immediately. Each role would need to merge back to main before the other roles can see their changes. This is worse than our current model where all three roles work on main and see each other's commits via `git pull --rebase`.
- Briefs written by Silas to `product-manager/briefs/` would not be visible in Wren's worktree until merged. This breaks our primary coordination mechanism.
- State file updates (e.g., Silas updates `system-architecture.md`) would not propagate to other worktrees without explicit merge steps.
- The workflow engine assumes all roles operate on the same filesystem tree.

**Verdict**: Worktree sessions solve a problem we do not have (file collision) while breaking the mechanism we depend on (shared filesystem for briefs and state files).

### Option B: Subagent fan-out (single session, 3 subagents)

**Mapping**: A single "orchestrator" session spawns 3 subagents, each configured with a role-specific system prompt, tools, and `isolation: worktree`.

**What works**:
- Single point of control. Jeff talks to one session.
- Subagents can run in parallel (background mode).
- Each subagent gets worktree isolation for safe file edits.

**What breaks**:
- Subagents cannot spawn other subagents. Our roles need to send briefs to each other, which implies multi-turn workflows that a subagent's limited context cannot support.
- Subagent results return to the parent session's context. Three roles producing substantial output would exhaust the parent's context window quickly.
- Subagents do not inherit the parent's conversation history. Each role starts fresh with only its system prompt. Our roles depend on rich, accumulated session context (state files, recent decisions, ongoing threads with Jeff).
- Subagent worktrees are temporary. The same isolation vs. shared-filesystem tradeoff applies.
- No inter-subagent communication. Subagents report back to the parent only. Our roles need peer-to-peer coordination (Silas sends brief to Kade directly).
- Our CLAUDE.md files are 500+ lines each with role-specific operational procedures. A subagent system prompt has much less room for this.

**Verdict**: Subagent fan-out forces our rich, stateful, long-running role sessions into a thin, ephemeral subagent model. The context window constraints alone make this unworkable for our use case.

### Option C: Agent teams

**Mapping**: Jeff starts a lead session, which spawns Silas, Wren, and Kade as teammates. Each teammate is a full, independent Claude Code instance with its own context window.

**What works**:
- Each teammate is a full Claude Code session. This matches our current model most closely.
- Teammates can message each other directly (like our Slack channel).
- Shared task list maps to our Vikunja kanban board conceptually.
- Each teammate loads CLAUDE.md from its working directory.
- Jeff can talk to individual teammates directly (like our 1x1 pattern).
- Split-pane display mode gives visibility into all roles simultaneously.

**What breaks or changes**:
- Agent teams are explicitly experimental and disabled by default. Known limitations include no session resumption, slow shutdown, and one team per session.
- The coordination model replaces our briefs/Slack/workflow stack with a built-in task list and mailbox. We would lose our audit trail, brief chain, and activity.md logging unless we explicitly re-implemented them.
- Agent teams assume a lead-worker topology. Our model is peer-to-peer with Jeff as director. The "lead" concept does not map cleanly -- Jeff is the lead but is not an AI agent, and no single role is the lead of the other two.
- No session resumption means losing all teammate context if the session ends. Our model depends on session continuity through state files and next-session.md. Agent teams do not persist.
- File conflicts are explicitly called out as a risk -- "Two teammates editing the same file leads to overwrites." We mitigate this with directory ownership, but activity.md and shared files would still be at risk.
- Token cost scales linearly with teammates. Three full Opus sessions running in parallel would burn tokens 3x faster than our current sequential model.
- All teammates start with the lead's permission settings. Our roles have distinct permission profiles (Silas/architect vs. Kade/engineer permissions differ significantly).
- The mailbox/task system is separate from our Vikunja board, Slack channels, and brief protocol. Running both systems would create coordination confusion.

**Verdict**: Agent teams are the closest conceptual match, but the feature is immature, the coordination model conflicts with our established protocol, and the cost/persistence tradeoffs are significant.

---

## 3. What Would Change in Our Architecture

If we adopted any of these mechanisms, the following would need to change:

| Current mechanism | Would need to... |
|---|---|
| Briefs protocol | Move to agent team mailbox OR be re-implemented within worktree merge workflow |
| Slack channels | Be replaced by agent team messaging (or duplicated) |
| Workflow engine | Be replaced by agent team task list (or duplicated) |
| State files (shared reads) | Need explicit merge/sync step between worktrees |
| Activity.md | Need conflict resolution strategy for concurrent appends |
| Permission profiles | Need per-teammate configuration (not currently supported in agent teams) |
| Session continuity | Need alternative persistence -- agent teams do not survive session end |
| CLAUDE.md generator | Would work as-is (files exist on disk, loaded by each session/teammate) |
| Chorus context index | Would work as-is (SQLite DB is read-only from role perspective) |
| Kanban board (Vikunja) | Would compete with agent team task list |

---

## 4. Risks

### Merge conflicts
Our roles occasionally write to the same files (activity.md, shared meeting docs). Worktree isolation makes this worse by deferring conflicts to merge time instead of surfacing them immediately. Our current "work on main, pull frequently" model handles this acceptably.

### Lost context
Agent teams do not survive session end. Our model explicitly designs for session discontinuity (next-session.md, state files, briefs). Adopting agent teams would trade our battle-tested persistence model for an experimental one with no persistence.

### Role boundary violations
Subagent worktree isolation does not prevent a role from reading or writing files outside its directory. The isolation is at the git-branch level, not the filesystem-permission level. Our PreToolUse hooks and permission profiles provide stronger role boundaries.

### Cost amplification
Three parallel Opus sessions consume tokens at 3x the rate of one session. Our current model is inherently sequential (Jeff works with one role at a time), which is more cost-efficient. Parallelism only helps if Jeff is not the bottleneck -- and Jeff IS the bottleneck by design (he directs, approves, and makes taste calls).

### Coordination overhead
Anthropic's own documentation states: "Agent teams add coordination overhead and use significantly more tokens than a single session. They work best when teammates can operate independently." Our roles are intentionally interdependent -- that is the point of the team architecture.

### Feature maturity
Agent teams are explicitly experimental. The documentation lists 8 known limitations including no session resumption, one team per session, and fixed lead assignment. Building our architecture on an experimental feature would create fragility.

---

## 5. What Stays the Same Regardless

These elements of our architecture are orthogonal to worktree/agent-team adoption:

- **CLAUDE.md per role** -- loads correctly in all three mechanisms
- **Chorus context index** -- read-only, works from any session
- **Git commit protocol** -- role-prefixed commits work in any branching model
- **Kanban board** -- external service, accessible from any session
- **Operational scripts** (app-state.sh, session-start.sh, etc.) -- filesystem tools, work from any working directory

---

## 6. Recommendation: Wait

**Do not adopt worktree isolation, subagent fan-out, or agent teams for the multi-role architecture at this time.**

### Why wait:

1. **Our current model works.** Three separate sessions with brief-based coordination, shared filesystem, and single-branch git is simple, auditable, and battle-tested through 3+ weeks of daily use.

2. **The problem these tools solve is not our problem.** Worktree isolation prevents file collisions between parallel writers. Our roles already have directory ownership. Agent teams coordinate parallel workers. Our roles are sequential by design (Jeff is the serial bottleneck).

3. **Agent teams are experimental.** The feature lacks session resumption, per-teammate permissions, and persistence -- all things our architecture requires.

4. **Cost math is unfavorable.** Three parallel Opus sessions cost 3x one session. Jeff works with one role at a time. Parallelism buys nothing when the human is the serial constraint.

5. **Our coordination stack would be disrupted.** Briefs, Slack, workflow engine, activity.md, and the Vikunja board form a cohesive coordination layer. Agent teams would replace parts of it with a less mature system.

### When to revisit:

- **Agent teams graduate from experimental** and gain session persistence, per-teammate permissions, and custom CLAUDE.md per teammate.
- **Jeff's workflow changes** to include parallel role work (e.g., "I want Silas and Kade working on different things simultaneously while I talk to Wren").
- **The Clearing evolves** to support persistent multi-agent sessions. The Clearing already provides the real-time multi-role alignment we want -- extending it with session persistence and structured handoffs may be more natural than adopting agent teams.
- **Subagents gain inter-agent communication** and richer context management. If subagents could message peers and maintain state across invocations, the fan-out model becomes more viable.

### Incremental experiments to consider:

1. **Kade-as-subagent for focused tasks**: When Silas needs Kade to build something specific (e.g., "implement this ADR"), Silas could spawn a Kade subagent with `isolation: worktree` for the implementation, review the result, and merge. This is a narrow use case that does not require full architecture change.

2. **Worktree for safe experimentation**: Any role could use `--worktree` for exploratory work that might need to be discarded (spike implementations, risky refactors). This is the intended use case for worktrees and does not conflict with our architecture.

3. **Agent team for demos**: A single session with 3 teammates could be used for demo purposes (showing Jeff all three roles working simultaneously) without replacing the production workflow.

---

## Sources

- [Claude Code Common Workflows -- Worktree documentation](https://code.claude.com/docs/en/common-workflows)
- [Claude Code Subagents documentation](https://code.claude.com/docs/en/sub-agents)
- [Claude Code Agent Teams documentation](https://code.claude.com/docs/en/agent-teams)
- [Anthropic Engineering -- Building a C compiler with parallel Claudes](https://www.anthropic.com/engineering/building-c-compiler)
- [ccswarm -- Multi-agent orchestration with worktree isolation](https://github.com/nwiizo/ccswarm)
- [Claude Code Git Worktree Support analysis](https://supergok.com/claude-code-git-worktree-support/)
- [Boris Cherny (Anthropic) -- Worktree announcement](https://www.threads.com/@boris_cherny/post/DVAAoZ3gYut/)
