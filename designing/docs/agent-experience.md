# Agent Experience (AX)

## What AX Is

Agent Experience is developer experience for AI roles. It's the answer to one question: **can an agent complete a basic operation on the first attempt, without workarounds?**

DX for human developers means: the CLI works, the docs are accurate, the error messages are helpful, the happy path is paved. Nobody argues with this. Every platform team in the industry measures it.

AX is the same thing, applied to agents operating inside a coordination system. The operations are different — querying a triplestore, committing to a shared repo, nudging another role, running a gate check — but the principle is identical: **the default path should work.**

## Why It Matters

When AX is bad, three things happen:

1. **Agents build workarounds.** Retry the grep. Add a synthesis paragraph to pass the edit gate. Check if we're behind remote before pushing to dodge the rebase. Skip the failing test. Every workaround means the underlying problem never gets fixed.

2. **Jeff becomes the debugger.** When an agent spends 5 minutes arguing that Fuseki data is missing because it's hitting port 3031 instead of 3030, Jeff pays the debugging tax. When a commit can't push because another role left dirty files, Jeff untangles the working tree. His attention is the most expensive resource in the system, and bad AX burns it on operational friction instead of creative work.

3. **Errors go silent.** A test that silently skips isn't a test. A route that returns 404 because the build is stale isn't monitored. A port number that's wrong in 7 files across 3 "fix" attempts isn't fixed. Bad AX means agents don't verify, and without verification, errors accumulate invisibly until Jeff trips over them.

## The Current State

### What agents fight on every prompt

| Friction | What happens | Workaround agents use | Cost |
|----------|-------------|----------------------|------|
| **Hook cascade on Grep** | First grep attempt blocked by compound context injection. Must retry. | Retry every grep. Accept the wasted turn. | 2x latency on every search, context pollution |
| **Synthesis gate on Edit** | Edit blocked until agent writes "Prior work / Current state / Approach" paragraph | Write boilerplate synthesis before every edit | Extra tokens, breaks flow, doesn't actually prevent bad edits |
| **Dirty working tree on push** | `git pull --rebase` fails when another role has unstaged changes | Ask Jeff to intervene, or check if behind remote first | Jeff becomes orchestrator, push sometimes just fails |
| **Port confusion (Fuseki 3030 vs 3031)** | Wrong port hardcoded in 7+ files across 3 "fix" attempts | Agents guess, get it wrong, debug for 5 minutes | Jeff watches the same failure repeat for weeks |
| **Silent test skips** | Integration tests skip when Vikunja is unreachable, report as "passed" | Nobody notices. Tests "pass" without running. | False confidence. Bugs ship. |
| **Stale builds** | `dist/` out of sync with `src/`. Routes exist in source, 404 in production. | Rebuild manually when someone notices | Features silently broken for days (interaction-patterns: 3+ days) |
| **Blast radius gate** | Card can't move to WIP if blast radius check returns 0 files | Add files to description, change card type, fight the gate | 5 minutes to start working on a doc (this card, right now) |

### What agents don't do

- **Verify after mutation.** Edit a file, don't check it compiled. Fix a port, don't grep for remaining references. Deploy, don't curl the endpoint.
- **Read error signal.** Logs exist. Test output exists. 404s exist. Agents build forward instead of reading what the system is already telling them.
- **Measure their own friction.** No agent tracks how many retries a session takes, how many workarounds it uses, or how much time is spent on operational friction vs. productive work.

## What Good AX Looks Like

### Principle: Zero-retry operations

Every basic operation — grep, edit, commit, push, query, deploy — should succeed on the first attempt. If it doesn't, that's a bug in the platform, not a skill gap in the agent.

**Before (current):**
```
Agent: grep for "product-manager"
Hook: BLOCKED — compound context injection
Agent: (reads context, retries)
Hook: OK
```

**After (good AX):**
```
Agent: grep for "product-manager"
System: (injects context AND returns results in one pass)
```

### Principle: Self-validating actions

Every mutation should verify its own result. Not as a discipline agents remember to follow, but as a property of the tool.

**Before (current):**
```
Agent: edit port 3031 → 3030 in harvest-media.sh
Agent: commit
Agent: say "done"
(6 other files still say 3031)
```

**After (good AX):**
```
Agent: edit port 3031 → 3030 in harvest-media.sh
Tool: (automatically greps for remaining 3031 references)
Tool: "Warning: 6 other files contain 3031 — fix those too?"
```

### Principle: Single source of truth, machine-readable

Port numbers, role-to-directory mappings, service endpoints — these should be in one place that tools read at runtime, not scattered as string literals in docs, scripts, CLAUDE.md, and role memory.

**Before (current):**
- Fuseki port in: CLAUDE.md, TEAM_PROTOCOL.md, harvest-media.sh, app-state.sh, fitness-test-template.md, 4 archived briefs, 3 Silas docs
- Each is a chance to be wrong

**After (good AX):**
- Fuseki port in: `config/services.json` → `{ "fuseki": { "port": 3030 } }`
- Everything else reads from that file

### Principle: The paved road is the only road

Agents shouldn't need to know workarounds. If the blast radius gate blocks a doc card, the gate is wrong — docs don't have blast radius. If `git pull --rebase` fails because another role has dirty files, the commit tool should handle that. If a test silently skips, the test runner should flag it.

**The test for good AX:** Can a brand-new agent session, with no accumulated workaround knowledge, complete a basic task cycle (read → build → test → commit → push → verify) without Jeff intervening?

Right now the answer is no.

## AX in the Chorus Model

In Jeff's 5-layer diagram:

| Layer | AX responsibility |
|-------|------------------|
| **DSL / Org / Team** (Loom) | Decisions, roles, practices are legible to agents — not just humans |
| **Interaction / Attention** | Nudges land. Chats don't stall. Gemba works without manual TTY hunting |
| **Operating Model / Pipeline** | Commit-push-deploy works atomically. Gates don't block valid work. Tests report honestly. |
| **Observability** | Agents can read their own error signal. Logs, alerts, dashboards are agent-queryable. |
| **Infra / Home Cloud** | Ports, endpoints, paths are discoverable at runtime, not memorized. |

AX is vertical — it touches every layer. It's closest to Spine and Pulse: an invisible property that everything depends on, and that nobody notices until it breaks.

## Measuring AX

Three metrics, all observable from session logs:

1. **Retry rate** — How many tool calls are retried per session? Target: 0.
2. **Jeff interventions** — How many times does Jeff have to unblock an operational issue? Target: 0 after session start.
3. **Time-to-first-productive-action** — From session start to first code edit or meaningful output. Current: 5-10 minutes of context loading and state reading. Target: under 60 seconds.

These aren't aspirational. They're diagnostic. Measure them, and the improvement priorities become obvious.
