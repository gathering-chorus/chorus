# ADR-055: Harness Dependency Boundary — Portable Core vs. Claude Code Adapter

**Status**: Proposed (drafted 2026-08-03, Jeff's go in-session)
**Context threads**: Jeff 2026-06-20 ("if we build a system for another party that system must NOT inherit all of our toolchain"), Jeff 2026-08-03 ("what kind of hard dependencies does chorus have on claude code")

## Context

Chorus today runs on one harness: Claude Code, on macOS, against the Anthropic API. That was the right build order — model-first, prove-by-running, one team as the living testbed. But the dependency surface has never been named as an architectural boundary, which means every new mechanism gets to choose (implicitly, silently) whether it couples to the harness or to the portable core. Eleven months in, the coupling is real but not uniform, and if a second-party deployment ever becomes real, the cost of the split will be set by decisions we are making now without noticing.

Four dependencies get conflated in casual discussion and must be separated:

1. **Claude Code the harness** — hook lifecycle, session model, skills, CLAUDE.md, headless `claude -p`
2. **The Anthropic API** — the models themselves
3. **macOS** — osascript injection, launchd, TCC-bound cdhashes
4. **Open protocols we happen to consume via Claude Code** — MCP

Swapping any one of these is a different project with a different cost. This ADR fixes the vocabulary and draws the boundary.

## Dependency inventory (as-is, 2026-08-03)

### Tier 1 — Deep: Claude Code contracts are load-bearing

| Mechanism | Harness contract it consumes |
|---|---|
| chorus-hooks daemon + shim (all gates: canonical_write_guard, write_scrubber, bouncer, membrane refusal plumbing, search-hierarchy guard, ICD gate) | Hook events (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`) + the permission-response JSON schema |
| Context injection (boot envelope, per-prompt chorus-context, memory inject) | `additionalContext` on SessionStart / UserPromptSubmit |
| Roles as sessions (registry `~/.chorus/sessions/`, nudge delivery, chorus-inject, gemba tailing, attention contract observability) | Long-lived interactive CLI session with a tty; also macOS-coupled (osascript) |
| Skills (`/pull`, `/demo`, `/clearing`, …) + generated CLAUDE.md role fragments (claudemd-gen) | Skill loader + CLAUDE.md conventions |
| Gate runners inside werk-demo | Headless `claude -p` as a compute primitive |

### Tier 2 — Medium: open protocol, single client

| Mechanism | Note |
|---|---|
| MCP surface (werk verbs, cards, nudge, principles, logs tools) | MCP is an open standard; any MCP client can speak it. Today the only wired client is Claude Code. This is the designed portability seam. |

### Tier 3 — Portable core: zero harness dependency

The spine (JSONL), OWL model + Fuseki + owl-api, board (Vikunja + `cards` CLI), Loki/Grafana/Prometheus, git worktree werk lifecycle, SQLite index + Lance, and — critically — **the Rust werk verbs, which are standalone binaries**. MCP is their skin, not their body; a different runtime can exec them directly.

## Decision

1. **Name the boundary.** Chorus = **portable core** (Tier 3) + **protocol surface** (Tier 2, MCP) + **harness adapter** (Tier 1). The Tier-1 layer is *our Claude Code adapter*, and is expected to be rewritten per-harness in any second-party scenario. It is not a defect that it exists; it is a defect only when core logic leaks into it.

2. **Placement rule (the enforceable part).** New logic defaults into the portable core or behind the MCP surface. A mechanism may live in the adapter only when it *inherently* consumes a harness contract (intercepting tool calls, injecting context, driving a session). Business rules, state, and verdicts stay out of the adapter: a hook may *enforce* a rule, but the rule's data and decision logic belong to core (the membrane is the worked example — surface list and context contract in a committed schema + shared module; the hook layer only carries the refusal).

3. **Verbs stay standalone.** Werk verbs remain independently executable binaries. No verb grows a hard requirement on being invoked via MCP or on Claude Code env being present. (`DEPLOY_ROLE`/`CHORUS_ROLE` env is an attribution *input*, fine; a `claude`-binary requirement is not.)

4. **Anthropic API ≠ Claude Code.** Model dependency is accounted separately. Headless-gate compute (`claude -p`) is the one place both couple at once; any future harness swap must re-source that compute (Agent SDK, direct API, another runner) — flagged here so it is costed, not discovered.

5. **No speculative abstraction.** We do NOT build a second adapter now, do NOT wrap the hook API in an indirection layer, do NOT port anything. One harness, named honestly, beats two harnesses half-supported (simplicity-is-strength; spike-before-building applies if/when a second party is real).

## Consequences

- "Could another party run this?" now has a costed answer: ship Tier 3 + Tier 2; budget the Tier-1 adapter rewrite (hooks, injection, session/nudge transport, skills packaging) per target harness; re-source gate compute.
- Review question at `/gate-arch` for new mechanisms: *"does this belong in the adapter, and if so, is it thin?"*
- The macOS coupling (osascript, launchd, TCC) rides the same boundary and is inventoried in ADR-050's CMDB scope; it is a separate swap cost from the harness.
- Risk to watch: the session-model assumption (role = interactive terminal session) is the hardest thing to port and the easiest place to accrete more coupling — nudge/attention mechanisms should prefer spine/API transports over tty transports where equivalent (the #3700 typed-delivery direction is already the right vector).

## Related

- ADR-050 (infra/toolchain CMDB) — inventory substrate for Tiers 1/3
- ADR-026 / ADR-053 — quality layers; gate compute is the `claude -p` consumer
- DEC-093 (all programmatic APIs on chorus-api :3340) — the portable API front door
- #3615 (test/prod membrane) — worked example of rule-in-core, enforcement-at-hook
