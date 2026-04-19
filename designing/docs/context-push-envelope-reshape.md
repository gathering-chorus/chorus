# Push Envelope Reshape — Manifest + Orientation

**Silas, 2026-04-19. Under #2234 Step 6. Sibling: `context-endpoint-schemas.md`. Defines the reshaped UserPromptSubmit inject envelope after pull endpoints exist.**

## Thesis

The push envelope today (`<context-synthesis>` block assembled in `context_inject.rs`) is ~8–16KB of pre-synthesized Pulse + Spine + Athena + Hybrid-search + Memory blob injected on every user prompt. The design promise was "the agent will read this." The measured reality (session 2026-04-19): agents routinely form claims that diverge from data actually in the envelope. Delivered memory, not consumed.

Once pull endpoints exist under `/api/chorus/context/*`, the push envelope's job changes. It stops being a pre-digested answer cache and becomes **a manifest + a one-glance orientation signal**. Agents reach for the API when forming claims; the envelope tells them where to reach.

Target byte budget: **~2KB** (from ~16KB). Target per-turn latency: **~50ms** (from ~720ms pre-#2231, ~120ms post-#2231). Consumption rate: agents cite pulled endpoints in responses, not pre-synthesized blobs.

## Current Envelope (baseline)

```
<context-synthesis>
Keywords: <query-terms>

## Pulse
  health: ok (failures=0, warnings=2)
  wip_cards: 2
    #2218 [Silas] Codesign chorus-hook-shim + chorus-inject with stable identifiers
    #2234 [Silas] Move chorus API from attic to workbench
  role silas: state=building card=#2234
  role wren:  state=building card=#2230
  role kade:  state=observing gemba=silas

## Spine (10 recent events)
  [10:14:32] silas → card.demo.started
  [10:13:58] wren → card.pulled
  ... 8 more ...

## Athena
  [full domain descriptor for current working domain — 1-3KB]

Chorus hybrid (5 hits):
  [2026-04-19] silas — "..."
  ... 4 more ...

Memory (3 hits):
  ... memory file snippets ...

MANDATORY: You MUST reference this context before responding...
</context-synthesis>
```

**Problems:**
- Volume. 8–16KB injected every turn. Agent skips past.
- Duplicates. Pulse contains WIP count + top cards. Spine contains events including card.pulled. Overlap buries salience.
- Pre-synthesized. If the agent wants fresh data mid-response, there's no URL to hit — the data is a text blob, not a query point.
- "MANDATORY" framing. Directive-tone without enforcement. Doesn't change behavior; agents internalize "it's always there, ignore the wrapper."

## Reshaped Envelope (target shape)

```
<chorus-context timestamp="2026-04-19T10:15:00-04:00" role="silas">

You are silas. You are currently building #2234 (Move chorus API from attic to workbench).

Pulse (at glance):
  health: degraded · 1 failure · 2 warnings
  team WIP: 2 · your WIP: 1 · team gemba: kade→silas
  index freshness: ok (last=2m ago)

Pull-first rule. When you are about to form a claim about current state (board, roles, health, events, quality, coverage), do not infer from this block. Query the relevant endpoint and cite its timestamp in your response.

Context endpoints (GET; response carries envelope.source + envelope.timestamp):
  /api/chorus/context/board/wip           — current WIP; ?role=X to scope
  /api/chorus/context/board/next          — Next cards ordered by priority; ?role=X to scope
  /api/chorus/context/roles               — all three roles, state, card, gemba, lastActivity
  /api/chorus/context/health              — system health; failure/warning counts + per-check detail
  /api/chorus/context/alerts              — firing alerts
  /api/chorus/context/spine?limit=N       — recent spine events
  /api/chorus/context/quality/summary     — current quality signals
  /api/chorus/context/coverage?domain=D   — coverage by domain

Knowledge endpoints (canonical truth):
  /api/chorus/knowledge/domains           — list; ?step=X to scope
  /api/chorus/knowledge/domains/{name}    — full domain detail
  /api/chorus/knowledge/search?q=...      — graph + FTS

Memory endpoints (cross-session):
  /api/chorus/memory/sessions             — session index
  /api/chorus/memory/briefs?to=X          — briefs directed at role X
  /api/chorus/memory/decisions?domain=D   — decision records

Actions (POST; mutate):
  /api/chorus/actions/spine-event         — emit spine event
  /api/chorus/actions/reindex             — trigger reindex

Cite what you read: "per /api/chorus/context/board/wip?role=silas @ 10:15, silas has 2 WIP."
</chorus-context>
```

**Shape changes:**
- Opening attributes (timestamp, role) on the tag itself — parseable.
- Identity sentence — "You are silas. You are currently building #X." One line, load-bearing.
- **Orientation only, no pre-digest.** Pulse-at-glance is 4 lines of one-line facts, not the full structured pulse block. Just enough to orient without pre-answering.
- **Pull-first rule** — explicit directive that claims come from pulls, not from this block. Cites the citation shape consumers should use.
- **Endpoint manifest** — grouped by sub-domain (Context / Knowledge / Memory / Actions). Each line: URL + one-line purpose. Agent scans it like a menu.
- **No pre-synthesized data blocks.** No Spine tail, no Athena dump, no hybrid-search hits, no memory-file snippets. Those move to endpoints.
- **No "MANDATORY" theater.** The pull-first rule names the contract without moralizing.

**Byte budget (estimate):**
- Orientation (4 lines): ~250 bytes
- Pull-first rule: ~250 bytes
- Endpoint manifest (15 lines): ~1500 bytes
- Wrapper + closing: ~50 bytes
- **Total: ~2KB.**

## Orientation Band Rules

The orientation band answers "where am I in the Werk right now" in <300 bytes. Rules:

1. **Identity first.** "You are silas. You are currently building #X." If no WIP, "You are silas. You have no WIP card."
2. **Health one-liner.** Three states: `ok`, `degraded · N failure · M warnings`, `down · critical`. No detail in the band; consumer reads `/health` for detail.
3. **Team state one-liner.** "team WIP: N · your WIP: M · team gemba: X→Y" (where X→Y means role X is gemba-ing role Y, or `none` if no active gemba).
4. **Index freshness.** `ok (last=Nm ago)` or `stale (last=Nm ago)` — signal whether pulled data from Memory/Knowledge paths will be fresh.

That's it. Four lines. Everything else goes to pull endpoints.

## Transition Strategy

Cannot flip from current envelope to reshape in one commit — agents are trained on the current shape. Phased:

1. **Phase 1 (behind flag):** `CONTEXT_PUSH_MODE=manifest` env var on chorus-hook-shim. When set, inject emits the reshaped envelope. Default stays current shape. Run in parallel; measure per-role.
2. **Phase 2 (per-role opt-in):** one role at a time (start Silas — I'm the driver on #2234). Live for one full working day. Watch for hallucinations / missed claims. If claims start citing pull endpoints by URL, the shape is working.
3. **Phase 3 (default flip):** manifest becomes the default. Old shape behind `CONTEXT_PUSH_MODE=legacy` for debug.
4. **Phase 4 (remove legacy):** old envelope assembly code retired after 1 week of default-manifest with no regressions.

Each phase is small and reversible. Envelope reshape is the highest-risk change in #2234 because behavior change is mediated by the agent's interpretation — a mis-shape trains worse habits.

## Measurement

- **Byte size.** Before vs. after, per inject. Target <2KB.
- **Per-turn latency.** Target <50ms for the envelope path (#2231 caching covers the old synthesis; post-reshape there's almost nothing to synthesize).
- **Consumption signal.** Grep role session JSONL for claims that include `/api/chorus/context/*` citations. Claims-with-citations / claims-about-state ratio is the metric. Target: >80% of state-claims in responses cite a pull endpoint with a timestamp.
- **Hallucination audit.** Periodic manual audit (Jeff + Wren) of 10 random responses per day — does each state-claim match current API state? Target: 0 divergences in a 7-day window.

## Interaction With #2231

#2231 cached the pulse+spine+athena fetches in the daemon to reduce per-turn cost. The reshape removes most of those fetches from the inject path entirely — caches go dead. That's acceptable; caching a 2KB envelope hardly matters. After reshape, #2231's caching code in context_inject.rs can be simplified or removed (cleanup card).

## Open Questions

Two design choices I'd normally decide myself but are worth flagging:

1. **Does `<chorus-context>` wrap or does Claude's stock context-block wrap?** Current: custom `<context-synthesis>` tag. Proposed: `<chorus-context>`. Marginal; pick one and be consistent.
2. **Should the manifest be static (same set for every turn) or personalized (endpoints relevant to the role's current card)?** Static is simpler and predictable; personalized is denser and more relevant but adds synthesis cost we're trying to reduce. Lean **static**. Personalization can be a later optimization if needed.

## Next Steps (per #2234 Implementation Outline)

- [x] **Step 1** — endpoint inventory audit
- [x] **Step 2** — minimum Context endpoint schemas
- [ ] **Step 3** — common envelope impl (Kade, Services vertical)
- [ ] **Step 4** — data correctness: pick three staleness sources, fix one
- [ ] **Step 5** — presentation: reshape three worst-shaped current responses
- [x] **Step 6** — push envelope reshape design (this doc)
- [ ] **Step 7** — demonstration: role cites `/context/*` endpoint in response

Step 3 (Kade's) is the gating dependency for 4–5–7. Step 6 design complete here; implementation is part of Step 3 scope (Kade writes the new inject; I'll collaborate on exact string shape).

## References

- `context-service-design.md` — parent
- `context-endpoint-schemas.md` — Step 2 output; the endpoints the manifest points at
- `context-api-endpoint-audit.md` — Step 1 output
- `platform/services/chorus-hooks/src/hooks/context_inject.rs` — current push-model code (to be reshaped)
- #2231 — the caching work that composes with this
- #2234 — the card
