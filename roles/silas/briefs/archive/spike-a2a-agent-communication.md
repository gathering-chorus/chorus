# Spike: Agent-to-Agent Communication Landscape

**Card**: #55
**Date**: 2026-02-17
**Author**: Silas (Architect)
**Status**: Complete
**Time-box**: 2 hours (research + synthesis)

---

## Question

What protocols, frameworks, and tooling exist for coordinating multiple AI agents? Should we adopt one, or is our bespoke protocol sound?

## Context

We operate 3 Claude Code sessions (Silas, Wren, Kade) as persistent role-based agents on a shared Git repo. Coordination today is manual: CLAUDE.md personas, brief routing via filesystem, Slack channels, a kanban board (Vikunja), and an activity log. Jeff wants to know if we're reinventing wheels — and whether the "Building" product (team protocol as a standalone method) should build on an existing standard.

---

## Landscape Summary

### Frameworks Evaluated

| Framework | What It Does | Maturity | Fit for Us |
|---|---|---|---|
| **Google A2A** | Inter-agent discovery + task routing over HTTP/gRPC | RC v1.0 (v0.3.0 released) | Poor — designed for agents that don't know each other across networks |
| **Anthropic MCP** | Shared tool access (model ↔ service) | Production | Partial — excellent for shared tools, not for agent coordination |
| **OpenAI Agents SDK** | In-process Python multi-agent orchestration | Production | Poor — OpenAI-locked, in-process only |
| **MS Agent Framework** | Async message-passing, AutoGen successor | Preview (GA Q1 2026) | Poor practical, moderate conceptual |
| **CrewAI** | Role-based agent crews with goals/backstories | Production | Poor practical, excellent conceptual match |
| **LangGraph** | Stateful agent graphs with checkpointing | Production | Poor practical, strong architectural influence |
| **Claude Code Agent Teams** | Native multi-session coordination | Experimental | Closest native fit, significant gaps |

### Key Findings

**1. No off-the-shelf solution exists for our pattern.**
Every framework assumes either (a) agents run in one process, (b) agents communicate over a network protocol, or (c) agents run synchronously together. We need persistent, asynchronous, role-differentiated agents coordinated by a human through a shared filesystem. Nothing covers this.

**2. Our bespoke protocol is architecturally sound.**
The patterns we use map cleanly to formal framework concepts:
- CLAUDE.md = Agent Card (A2A) / Role definition (CrewAI)
- Briefs = Task messages (A2A) / Inter-agent handoffs (OpenAI SDK)
- Activity log = Event stream (LangGraph) / Audit trail
- Board = Task lifecycle (A2A) / State graph (LangGraph)
- Slack = Pub/sub signaling

**3. Claude Code Agent Teams is the closest native tool — but solves a different problem.**
Agent Teams coordinates parallel work on one task (synchronous, ephemeral). We coordinate role-based collaboration across sessions (asynchronous, persistent). Specific gaps:
- No persistent identity across sessions (our agents need to be Silas/Kade/Wren every time)
- No per-agent CLAUDE.md (all teammates share one — our agents need different role files)
- Synchronous only (all agents must run together — ours run at different times)
- No resumption (teammates lost on session resume)

**4. MCP is the most promising integration point.**
A custom MCP server could formalize our coordination layer — exposing the board, briefs, activity log, and Slack as structured tools that all three agents connect to. This standardizes what we currently do via shell scripts (board.sh, slack-post.sh, slack-read.sh).

**5. The "repo as shared state, commits as messages" pattern is validated.**
Anthropic's own engineering team built a C compiler with 16 parallel Claude Code sessions and 2,000+ sessions total. Their coordination: "The repo is the shared state and commits are the messages." This is exactly what we do.

**6. A2A's Agent Card pattern is worth borrowing.**
A machine-readable capability declaration per role (what it owns, what it can do, how to reach it) would make brief-routing more robust and could become a Building artifact.

**7. Industry gap = Building product opportunity.**
Anthropic's 2026 report calls out "human as orchestrator of AI agents" as a defining trend. No tooling properly supports it for CLI-based agents. Our protocol — if formalized — fills this gap.

---

## Assessment: What Should We Do?

### Option A: Adopt A2A (Rejected)
A2A solves inter-organizational agent discovery. Our agents share a filesystem. The transport layer, Agent Cards, and task lifecycle are overkill. The protocol overhead would slow us down without solving our actual problems (persistence, role differentiation, asynchronous handoff).

### Option B: Adopt Agent Teams (Not Yet)
Agent Teams is the right direction but too immature for our needs. We'd lose persistent identities, role-specific CLAUDE.md files, and asynchronous operation — all core to how we work. **Revisit when Agent Teams supports persistent roles and async operation.** Watch the experimental flag.

### Option C: Build MCP Coordination Server (Recommended — Phase 2)
Wrap our existing coordination patterns (board, briefs, Slack, activity) in an MCP server. Each agent connects to it instead of calling shell scripts. Benefits:
- Standardized interface (MCP is production-stable)
- All three agents get identical, reliable access to coordination tools
- The MCP server becomes a Building artifact (portable, documentable)
- No architecture change — same patterns, better tooling

**Not yet.** Current shell scripts work. This becomes valuable when we want Building to be portable or when coordination reliability becomes a friction point.

### Option D: Formalize Current Protocol (Recommended — Now)
Our protocol works. The gap isn't the protocol — it's the rigor of execution. What we should do now:

1. **Document the protocol as a standard** — not just in team-architecture.md, but as a formal spec that could be implemented by any agent framework. This IS the Building product seed.
2. **Add Agent Card–style declarations** to each role's CLAUDE.md — machine-readable capability section (owns, produces, consumes, channels).
3. **Instrument the protocol** — replace subjective rules ("update activity.md") with deterministic actions (hooks, scripts, or MCP tools that fire automatically).
4. **Define fitness functions** — measurable criteria for whether the protocol is being followed (e.g., "every brief has a corresponding activity.md entry within 1 session").

---

## Recommendation

**Short term (now):** Option D — formalize and instrument what we have. The protocol is sound; the execution needs tightening. Add machine-readable role declarations. Replace manual steps with automated ones where possible.

**Medium term (when Building becomes a product):** Option C — wrap coordination in an MCP server. This makes the protocol portable and testable.

**Long term (when Claude Code Agent Teams matures):** Option B — evaluate whether native agent teams can replace our bespoke coordination. Watch for: persistent identities, per-agent config, async operation.

**Explicitly not doing:** Option A. A2A is for a different problem space.

---

## Patterns Worth Borrowing

Even though we're not adopting any framework wholesale, these patterns from the landscape are directly applicable:

| Pattern | From | Application |
|---|---|---|
| Agent Card (capability declaration) | A2A | Machine-readable role section in CLAUDE.md |
| Task lifecycle states | A2A, LangGraph | Formalize card states: pending → in-progress → blocked → done |
| Checkpointing | LangGraph | Session state files already do this — make it explicit |
| Human-in-the-loop interrupts | LangGraph, CrewAI | Jeff's "working hours" rule = structured interrupt pattern |
| Role + Goal + Backstory | CrewAI | Our CLAUDE.md already does this — good validation |
| Repo as shared state | Anthropic C compiler | Validates our approach — commits are the canonical coordination medium |
| Handoff pattern | OpenAI SDK | Briefs are handoffs — formalize the contract (what must a brief contain?) |

---

## What This Means for Building

If Building becomes a standalone product, it's essentially: **a protocol specification for one human coordinating N AI agents through a shared repository, with role-based personas, asynchronous brief-based handoffs, and instrumented fitness functions.**

No existing framework packages this. CrewAI comes closest conceptually but is locked to in-process Python. The market opportunity is real — Anthropic's own report confirms the pattern is emerging, and the tooling gap exists.

The artifacts that become the Building product:
- Protocol specification (evolved from team-architecture.md)
- Role declaration format (Agent Card–inspired CLAUDE.md sections)
- Coordination MCP server (when built)
- Fitness functions and audit tooling (protocol-status.sh evolves)
- Case study: Gathering as the first project built this way

---

## Sources

- [Google A2A Protocol](https://a2a-protocol.org) — v0.3.0, Linux Foundation
- [Anthropic MCP](https://modelcontextprotocol.io) — Production, donated to Agentic AI Foundation
- [Claude Code Agent Teams](https://code.claude.com/docs/en/agent-teams) — Experimental
- [Claude Agent SDK](https://platform.claude.com/docs/en/agent-sdk/overview) — Python/TypeScript
- [OpenAI Agents SDK](https://openai.github.io/openai-agents-python/) — Successor to Swarm
- [MS Agent Framework](https://learn.microsoft.com/en-us/agent-framework/) — AutoGen successor
- [CrewAI](https://docs.crewai.com) — Role-based agent crews
- [LangGraph](https://docs.langchain.com/oss/python/langgraph/) — Stateful agent graphs
- [Building a C Compiler with Parallel Claudes](https://www.anthropic.com/engineering/building-c-compiler) — Repo-as-state pattern
- [Anthropic 2026 Agentic Coding Trends](https://resources.anthropic.com/2026-agentic-coding-trends-report)
- [parallel-cc](https://github.com/frankbria/parallel-cc), [claude-flow](https://github.com/ruvnet/claude-flow), [claude-swarm](https://github.com/affaan-m/claude-swarm) — Third-party coordination tools

---

— Silas
