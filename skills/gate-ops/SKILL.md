---
name: gate-ops
description: Operations gate — verify deploy safety, health checks, log flow, rollback path. Silas only. Fires at deploy, not code-complete.
user-invocable: true
---

# /gate-ops — Operations Gate

Fires at deploy time, NOT code-complete. Verifies the change can be run safely. **Silas only.**

## Arguments

```
/gate-ops <card-id>
```

## Owner Check

Only Silas can run this gate. If another role invokes it:
```
Ops gate is owned by Silas.
```
Exit — no checks run.

## Prerequisite

/gate-arch must have passed for this card. Check for `gate:arch-pass` card comment. If not found:
```
WARN: No gate:arch-pass found for this card. Run /gate-arch <card-id> first.
```
In pilot mode: warn but continue. In enforce mode (future): block.

## Applicability Check

Read the card with `cards view <card-id>`. Check the card type label.

- Doc-only / board-process / skill-only cards with no deploy: **SKIP** — "Ops gate not applicable."
- Cards that touch services, scripts, hooks, LaunchAgents, deploy paths: **RUN**.

If skipped, emit: `gate.ops.skipped` spine event. Exit.

## Automated Checks

Run `chorus-health` — it is the canonical ops fitness function (#2952). All checks live there.

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-health --verbose
```

chorus-health covers: service-health (caddy-edge, caddy-admin, gathering-app, chorus-landing, 10 borg pages, fuseki, chorus-api, loki, bridge), rollback-path, disk-health, spine-event-rate, MCP round-trip, Bedroom reachability, nudge-delivery, and more.

**Pass:** chorus-health exits 0 (no FAIL checks).
**Fail:** chorus-health exits 1 — print the failing checks from its output.

## No Manual Confirms

All ops checks are automated. No manual step.

## Result

Print summary:

```
## /gate-ops #<card-id>

  chorus-health: PASS | FAIL (N failures listed)

  VERDICT: PASS | FAIL
```

## On Pass

1. Emit spine event: `gate.ops.passed` with card ID
2. Add card comment: "gate:ops-pass — Silas"
3. Prompt: "Chain complete. Next: whoever is running /demo or /acp will see all 5 gate passes on the card and proceed." (#2222 — demo-caller is already watching the card; no nudge needed)

## On Fail

1. Emit spine event: `gate.ops.failed` with card ID and failing items
2. Print failing items. Do not nudge forward. Fix the ops issue first.
