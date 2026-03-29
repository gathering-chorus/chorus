# Spike: Headless Operations Agent (#178)

**Question**: Does Claude offer a headless operations solution for autonomous infrastructure monitoring?

**Date**: 2026-02-23
**Time-box**: 1 session
**Status**: Complete — research done, recommendation ready

## Findings

### 1. Claude Agent SDK (GA)
- **What**: Open-source (MIT) SDK for building custom AI agents using Claude
- **Languages**: Python and TypeScript
- **Architecture**: Same agent loop as Claude Code — tool use, reasoning, multi-step execution
- **Key capability**: Custom tool definitions, sandboxed execution, programmatic control
- **Cost**: Uses API credits (not included in Max subscription)
- **Effort**: Medium — need to define tools, build harness, deploy as service

### 2. `claude -p` Headless CLI Mode (Recommended Phase 1)
- **What**: Claude Code CLI accepts piped prompts — no interactive terminal needed
- **Usage**: `echo "check system health" | claude -p --allowedTools Bash,Read`
- **Cost**: Uses Max subscription ($200/mo fixed) — $0 marginal cost per invocation
- **Effort**: Low — bash scripts, launchd scheduling, existing tool permissions
- **Limitation**: Each invocation is stateless (no persistent memory across runs)
- **Key advantage**: Already installed, already authenticated, already has tool access

### 3. MCP (Model Context Protocol) Servers
- **What**: Extend Claude Code with custom tool servers
- **Relevance**: Could provide Claude with direct access to Prometheus, Loki, Vikunja APIs
- **Status**: Supported in Claude Code today

## Recommendation

**Phase 1 (Now)**: Use `claude -p` for headless operations. Build a `ops-agent.sh` script that:
1. Runs on launchd schedule (every 15-30 min)
2. Pipes a structured prompt with current system state
3. Uses `--allowedTools Bash,Read,Write` for investigation + remediation
4. Logs output to chorus.log for visibility
5. Can create board cards for issues found

**Phase 2 (When needed)**: Build a proper Agent SDK service if:
- We need persistent state across runs
- We need custom tool definitions beyond CLI capabilities
- We need real-time event-driven reactions (not polling)

**Phase 3 (Future)**: MCP servers for direct API access to Prometheus, Loki, Vikunja — eliminates shell-out overhead.

## Why `claude -p` First

| Factor | Agent SDK | `claude -p` |
|--------|-----------|-------------|
| Cost | API credits (~$0.03-0.10/run) | $0 (Max subscription) |
| Setup | Build harness, deploy service | Bash script + launchd |
| Tools | Custom definitions | Existing Claude Code tools |
| State | Persistent (custom) | Stateless per run |
| Time to ship | Days | Hours |

The Max subscription makes `claude -p` essentially free. At 30min intervals, that's 48 runs/day — well within subscription limits. Phase 1 validates the concept; Phase 2 adds sophistication only if needed.

## Risks

- **Subscription limits**: Claude Max may have rate limits on `claude -p` usage. Need to verify.
- **Stale context**: Each run starts fresh — no memory of previous findings. Mitigate with state files (like defect-poller pattern).
- **Runaway costs**: If Agent SDK is used in Phase 2, API costs could be significant. Set budget alerts.
- **Security**: Headless agent with Bash access needs careful permission scoping.

## Next Steps

1. Verify `claude -p` works with `--allowedTools` flag
2. Prototype `ops-agent.sh` with health check prompt
3. Test launchd scheduling
4. Define MCP server for Prometheus/Loki if Phase 1 proves valuable
