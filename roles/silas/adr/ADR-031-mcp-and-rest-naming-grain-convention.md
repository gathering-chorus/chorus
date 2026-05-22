# ADR-031: Resource / Verb / Grain Convention for Chorus Interfaces (MCP + REST)

**Status:** Accepted
**Date:** 2026-05-21
**Author:** Silas
**Cards:** #3028 (this ADR), #3025 (MCP tool consolidation that surfaced it), #3029 (pain board, first consumer of the validated query contract)

## Context

The interface surface grew without a convention and drifted into four problems, surfaced by #3025's registry-integrity weed:

- **37 MCP tools, 4 competing name patterns** — `chorus_cards_view` (resource_verb), `chorus_commit` (bare verb), `nudge_message` (verb-first), `card_add_jeff` (actor suffix), `logs_for_card` (prepositional).
- **"read" has 6 verbs** — get / view / list / lookup / query / status.
- **Grain ranges from atomic (`service_*`) to composite (`acp`)** with no rule for when a call may bundle.
- The **REST/Athena surface has the same drift** — v1 `/api/athena/tree`, `/ownership/:iri`, `/blast-radius/:iri` are three ad-hoc paths for what is one resource (`subdomains`).

A naming/grain convention is only real if something checks it; policy/principle/practice are voluntary unless gated (Jeff: teeth = the gate).

## Decision

One rule, two surface bindings.

1. **Shape.**
   - MCP: `chorus_<plural-resource>_<verb>` (e.g. `chorus_cards_get`). No bare verbs, no verb-first, no prepositions, no actor suffixes.
   - REST: resource-oriented path (`/api/<area>/<plural-resource>`), HTTP verb = operation, **query params = discriminators**, one source of truth per resource.
2. **Closed verb set:** `get` / `list` / `add` / `set` / `remove` + literal lifecycle verbs (`start` / `stop` / `deploy` / `rollback`).
3. **Grain:** one call = one operation on one resource. Two allow-listed exceptions: named transactions (`acp`, `pull_card`) and lifecycle families (`service_*`).
4. **Discriminators are arguments, not tools/paths.** `logs_for_card|trace|branch|query|recent_errors` → `logs_list(key, level)`; `card_add_jeff` → `cards_add(attribution=jeff)`; v1 athena tree/ownership/blast-radius → `subdomains_get(include=blast_radius)`.
5. **Properties vs. status carve-out.** `cards_set` owns every descriptive **property including labels** (validation lives inside the setter). **Status is NOT a property** — it is a state machine. Transitions go through `cards_move` (lanes) and the accept transaction (`Done`, which emits `card.accepted`). A generic setter must never write status, because that silently bypasses the demo/accept gate and the audit emit (the live bug: `cards_move`→Done skipped `card.accepted`).
6. **Enforcement = a CI name-test on tool names + REST paths** — the gate of record, not a loom-shelf document.

## Rollout

Breaking change to live callers (≥9 files + runtime callers + hooks `mcp_client.rs`), so:

1. Ship new names with old names as **deprecation aliases** (old delegates to new) — zero breakage on deploy.
2. CI name-test allow-lists the aliases as `deprecated`.
3. Migrate callers; then drop the aliases and the allow-list entries in a follow-up.

## Consequences

- ~37 MCP tools → ~26 (lifecycle `service_*` is the remaining irreducible block of 6).
- AC6 of the Athena v2 work (retire v1 `/api/athena/tree` + `/ownership/:iri` + `/blast-radius/:iri`, consolidate on v2 `/api/athena/subdomains`) cites this one rule across both surfaces.
- Validated query contract that falls out of this (recorded on #3029): query Loki by `{job=~".+"}` (file-tail label), never `appName` (docker-only); anchor on the event field (`|~ "event":"X"`), never a bare substring.
- gate-arch + gate-ops passed on #3025 (the consolidation that applies this).
