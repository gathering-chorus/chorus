# Friction Reconciliation — one real session, hand-count vs data (#3283)

**Session audited:** silas, 2026-06-10 08:54–13:25 Boston — deliberately the busiest available corpus (4 lands, one machine incident, the gh-401 outage, two classifier fights).
**Method:** every block/refusal/error the agent *experienced* was enumerated from the session transcript by the agent that lived it, then compared against what the data layer captured for the same window and role.

## Hand count (lived): ~27 felt friction events

| Class | Lived | Captured | Where |
|---|---|---|---|
| Hook blocks (memory-first, no-raw-kill, canonical-guard, synthesis gates) | 6 | **8 denies** ✓ | hooks.log / friction board |
| Claude Code **classifier denials** (bootout, bin-rm, token setenv, read-deny) | 4 | **0** ✗ | nowhere |
| **Harness refusals** (file-not-read Edit refusals, schema validation) | 5 | **0** ✗ | nowhere |
| MCP verb errors (401s, pipeline-fail, no-verdict refusal) | ~6 | **4** ◐ | mcp.tool.error (spine/Loki) |
| Shell-invoked CLI refusals (cards add flag-refusals ×4, agent-state exit 7) | ~5 | **0** ✗ | nowhere |
| Transport drops (response lost, server-side success) | 3 | 0 — honest | n/a (server truly succeeded) |

**Capture rate of felt friction: ~12 of 27 ≈ 44%.** (Corroborated by a second session the same day: Kade hand-counts ~15 shell refusals + 3 classifier denials, all equally uncaptured.) The undercount thesis (#3278) is confirmed, with the gaps named:

## Residual undercount, by cause

1. **Classifier denials are structurally invisible** — they happen inside the Claude Code harness before any tool runs; no chorus process ever sees them. Today they were the *highest-impact* blocks of the session (the pkill denial during the load-100 outage cost minutes of downtime). #3281 captured werk-demo refusals, not this class. Closing it needs a harness-side hook (PostToolUse/denial hook emitting to spine) — fill-card material.
2. **Harness refusals (file-not-read, schema errors)** — same class, same invisibility, lower impact.
3. **Shell-invoked CLI refusals** — narrower than first written (Kade's review, corroborated by his ~15 invisible shell refusals the same day): the werk-verb *binaries* already jsonl/spine on refuse regardless of invocation path; the gap is exactly the **cards CLI + bash scripts**, which exit 1 and emit nothing. Fix is a refusal-emit in those two surfaces — NOT "force everything through MCP," which today's daemon outage disproved as the robust half of that fork (the shell path was the working fallback all afternoon).
4. **Transport drops are honestly absent** — the server succeeded; the loss is client-side. Absence here is correct, but it means "Jeff saw a failure, data shows none" is *expected* for this class. Kade's #3320/#3323 shrank the class.

## The inverse finding: counted-but-not-felt

The same window logged **96 ambient warns** for silas (ops_awareness 59, prompt_pipeline 37) that the agent never saw — warns fire-and-allow invisibly. So the friction board simultaneously **overcounts felt friction** (ambient warns dominate by volume) and **undercounts hard blocks** (classifier/harness/CLI classes missing). Felt-friction ≠ counted-friction in both directions.

Also: lived block *text* does not map cleanly to data *module names* (e.g. the canonical-guard message vs the module label recorded) — #3282's labeling work should anchor on module names, not message text.

## Live addendum — the audit generated its own specimens

Filing this report's card-comment was itself blocked **twice** by the raw-process guard: the comment *text* mentioned the process-cleanup tool by name, and the hook pattern-matches the bare substring anywhere in a command — including inside a quoted card comment. Two more lived pure-friction events (bringing the session to ~29), zero of them captured, and the cleanest possible #3282 exhibit: a guard that cannot distinguish *doing* the dangerous thing from *writing about it*.

## Verdict (AC3)

Gap is **not ~0**; it is ~56%, with four named causes above. Items 1–3 are closable (one harness hook + one CLI refusal-emit); item 4 is correct behavior. #3282 should classify against the *captured* corpus while knowing the captured corpus skews toward soft warns and away from hard agent-side denials.
