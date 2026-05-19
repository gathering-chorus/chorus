# /card — File a Card (Jeff-Initiated)

When Jeff types `/card <title or natural args>`, file a card on the team kanban board immediately. The skill invocation IS the authorization — no bouncer fires, no approval ask lands in Jeff's terminal.

**Substrate rule:** /card is Jeff-initiated by definition. The agent runs `cards add` with `DEPLOY_ROLE=jeff`. The bouncer's job is to gate agent-proposed cards; /card invocations are explicitly out of scope.

## Arguments

Two shapes accepted, both work the same way at the substrate layer:

**Natural form:**
```
/card make a test card for silas to do X
/card retire the old crawler endpoint
/card silas: chorus-health alert needs retry policy
```

**Strict-args form (when Jeff wants precision):**
```
/card --owner silas --priority P2 --domain chorus --type fix "title text here"
```

## Step 0: Parse the args

Extract from the prompt body:

- **title** — the substantive description. Required. In natural form, the whole prompt body after `/card` IS the title (after trimming directive words like "make a card for", "file a card to", etc.).
- **owner** — infer from "for <role>" pattern in natural form (`for silas` → owner=Silas). In strict form, read `--owner`. Default: Wren (the invoking role's default).
- **type** — infer from title vocabulary:
  - "test card" / "test:" / "throwaway" → `chore`
  - "fix" / "broken" / "bug" / "regression" → `fix`
  - "swat" / "crisis" / "production-down" → `swat`
  - "feature" / "new" / verb forms like "add X" / "ship Y" → `new`
  - "improve" / "tune" / "enhance" → `enhance`
  - default: `chore` (Jeff-direct cards are usually housekeeping)
- **priority** — infer from urgency words ("urgent" / "P1" / "now" → P1; "soon" → P2; "eventual" / "later" → P3); default: `P3`
- **domain** — infer from `--domain` flag or context (mention of "athena", "loom", "werk", "borg", "clearing", "convergence", "chorus" → domain:chorus with that subproduct); default: `chorus`
- **sequence** — match the inferred subproduct (athena/loom/werk/borg/clearing/convergence) or default to `chorus`
- **origin** — `reflective` (Jeff chose this; not reacting to breakage). If title has "fix" / "broken" / "regression" → `reactive`.

If args are ambiguous, surface the inferred values to Jeff in a one-line preview before filing — but don't ask. He can override on the next message if wrong.

## Step 1: File the card

Invoke the dedicated MCP tool `chorus_card_add_jeff`. The tool spawns `cards add --quick` with `DEPLOY_ROLE=jeff` hardcoded in env — bouncer's `requireJeffApprovalIfAgent` returns immediately (`isAgent` is false), card lands directly. No marker, no freshness window, no detector.

```
mcp__chorus-api__chorus_card_add_jeff({
  title: TITLE,
  owner: OWNER,
  priority: PRIORITY,
  domain: DOMAIN,
  type: TYPE,
  origin: ORIGIN,
  sequence: SEQUENCE,        // optional
  subproduct: SUBPRODUCT,    // optional
})
```

That's the entire mechanism. One MCP call, one card. No bash fallback (agent sessions can't shell out to `cards add` directly anyway).

## Step 2: Report back

Print one line with the new card ID + the inferred axes Jeff can verify at a glance:

```
Filed #<id>: <title>
Owner: <owner>  Type: <type>  Priority: <priority>  Domain: <domain>  Sequence: <sequence>
```

If Jeff says any axis is wrong on his next prompt, update via `cards update <id> --<axis> <value>` (also with `DEPLOY_ROLE=jeff`).

## Hard rules

- **Always `chorus_card_add_jeff` MCP.** It's the only Jeff-attribution surface. The skill never falls back to bash or to the agent-attributed `chorus_cards_add` (which would fire the bouncer).
- **No bouncer.** `chorus_card_add_jeff` spawns the CLI with `DEPLOY_ROLE=jeff`. The bouncer's `requireJeffApprovalIfAgent` returns immediately. If the bouncer fires after a `/card` invocation, that's a substrate bug — reopen the card it landed against and fix.
- **No retry loop.** One invocation, one card. If the MCP errors, surface the exact error and stop. Don't retry without Jeff's signal.
- **Natural-form parsing is best-effort.** If the title contains ambiguity ("make a card for silas to fix X for kade") — pick the most-recently-mentioned role and ship. Jeff corrects on the next prompt if wrong.

## What /card replaces (#2996)

This skill is the single canonical path for Jeff-initiated cards. The earlier natural-language directive-detector approach (`card_directive_detector` Rust hook + `consumeFreshDirectiveMarker` bouncer-skip in sdk.ts, shipped in #2964) was retired in #2996. It was a tuning loop: the detector missed natural forms ("making a card" vs "make a card"), the freshness window was too short for agent compose-time (60s vs ~2min observed), and the two mechanisms had no middle. `/card` + `chorus_card_add_jeff` removes the ambiguity. Invocation IS intent; no detection layer needed. Aligns with `chorus:principle-no-competing-implementations`.

## When NOT to use /card

- When an agent (Wren/Silas/Kade) is filing a card from their own initiative — they go through the bouncer, not `/card`. The bouncer is the visibility surface Jeff wants for agent-proposed work.
- When Jeff approves a pending bouncer payload — that flows through the `card_approval_responder` hook and `handle_approval_request_all`. Different substrate, same outcome.
