# DEC-2311: Decompose "hook" into guard / injector / observer / scheduler / validator

**Date:** 2026-04-20
**Source:** Wren, during pair on #2311 (boot-time protocol contract rescope). Jeff: "can you decompose them into something more specific than 'hooks'" → "make that a dec/adr."
**Status:** Active
**Landing note:** Filed as markdown pending the loom-decisions API (#2318). Migrate via API when shipped. Loom owns decision-as-artifact per Jeff 2026-04-20.

## Decision

The word "hook" is retired as a primary design term. Hook mechanisms are named by their semantic responsibility:

- **Guard** — deny/allow decision on an event. PreToolUse blocking a tool call. Synchronous, fail-closed.
- **Injector** — writes context into the model's view. SessionStart returning `additionalContext`. One-way, pre-response.
- **Observer** — emits or logs without affecting flow. PostToolUse spine events, `chorus-log` emitters. Side-effect only.
- **Scheduler** — time-triggered, not event-triggered. Cron, launchd, `ScheduleWakeup`. Different lifecycle from event hooks.
- **Validator** — shape check at a boundary. Pre-commit, SHACL on TTL, schema checks, lint. Strong-API version of convention.

One hook implements exactly one of these responsibilities. A file that mixes two (e.g., a PreToolUse hook that both guards AND emits spine events) must be split or explicitly document both roles with separate code paths.

## Why

The protocol contract card (#2311) failed gate:product three times because "hook" collapsed three distinct responsibilities. Prose in CLAUDE.md ("read session-start first") was doing guard work; a Rust `session_init_gate` was doing injector work by proxy; spine events were doing observer work by assumption. Nobody knew which layer was load-bearing because the vocabulary hid the distinction.

The #2311 rescope worked because it separated the three jobs cleanly:
- SessionStart = **injector** (emits session-start payload on stdout as additionalContext)
- PreToolUse = **guard** (binary deny-all until .done marker written)
- `session.protocol.violation` = **observer** (spine event on gate hit)

Three distinct mechanisms, three distinct file paths, one AC item per responsibility. Jeff's "no competing implementations" principle (landed same day as loom-principle `chorus:principle-no-competing-implementations`) applies directly: each responsibility has one implementation, not N variants.

The five-way decomposition also surfaces jenga-shaped bugs. `/pair-heartbeat-check` (filed as #2317) is a **scheduler** reference that no implementation answers. `stories.md` edit-blocker (#2316) is a **guard** enforcing a contract whose **validator** (`write-story.sh`) doesn't exist. Same word "hook" hid both gaps.

## Enforcement

- Card descriptions and ACs that use the word "hook" must specify which kind. `[guard]`, `[injector]`, `[observer]`, `[scheduler]`, `[validator]` as prefix or inline qualifier.
- Code review checks: if a hook file implements two responsibilities, flag for split.
- When adding a new enforcement mechanism, name the responsibility before choosing the trigger (PreToolUse, SessionStart, etc.). Responsibility drives mechanism, not the other way around.
- Retire the word "hook" from headings and skill names where a more specific term fits. Transitional: existing docs may say "hook (guard)" or "hook (injector)" until rewritten.

## Strength tiers (orthogonal axis)

A guard/injector/observer/scheduler/validator can sit at any of three strength tiers:

1. **Machine-enforced** — harness refuses. Guards + injectors implemented as Claude Code hooks live here. Cannot be bypassed by model compliance.
2. **Test-enforced** — CI catches after-the-fact. Validators implemented as pre-commit checks, unit tests asserting a shape.
3. **Convention** — prose in CLAUDE.md, skill text. Weakest. Model may or may not comply.

Prefer tier 1 for every responsibility that has a claimable surface in the harness. Tier 3 (prose) is the jenga attractor and must be named explicitly when chosen.

## Related

- #2311 — boot-time protocol contract (canonical rescope that produced this vocabulary)
- #2317 — `/pair-heartbeat-check` scheduler with missing implementation
- #2316 — stories guard without matching validator/API
- #2314 — loom-principles API (needed to replace TTL hand-edit, which violates the "principle" being landed)
- `chorus:principle-no-competing-implementations` (loom-principle, 2026-04-20) — root principle this decomposition serves
- DEC-093 — domain endpoints (validator-class enforcement of API contract)
