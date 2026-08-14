# Chain-builder — the /effective cascade completion (Silas's leg)

Spec agreed by Silas + Wren, 2026-08-12 night pairing. This is the remaining code
leg after #3845 (routing, landed) and the data leg (landed + verified below).

## What exists now (verified against the running system)

- `/effective/:node/:key` serves (Wren #3845 — routing only).
- `effective_response` (owl-api lib.rs:~2150) builds a **one-element** chain,
  `ScopeKind` **hardcoded to Service**, reads **one fixed graph** (the route
  table's `instances_graph` — Wren routed /effective under the Property table).
- Data landed in `urn:chorus:instances`, verified — the resolver's exact fetch
  query returns it:
    - `property-response-word-cap-default-chorus` : propertyKey "response.word.cap",
      propertyValue "100", propertyValueType "int", propertyScope "global"
    - `value-stream-chorus  chorus:hasProperty  property-...-default`
    - `role-silas  chorus:partOf  value-stream-chorus`   (the anchor)
- NO role-silas override written — that value is Jeff's policy to set, not invented.

## Why /effective returns "no property sets key" for both nodes

TWO gaps, both at lib.rs:~2150, both mine:

1. **Single node.** Reads only the target node's own properties. role-silas has no
   own response.word.cap (only the partOf edge), so → unset. The stream default is
   invisible because nothing walks partOf.

2. **Single fixed graph.** Reads the Property table's `instances_graph`. But scope
   nodes span graphs:
     - value-stream-chorus, role-silas → `urn:chorus:instances`
     - cards-service (existing gate.enforced) → `urn:chorus:ontology`
   A single-graph read can never resolve properties for nodes that live elsewhere.

## The fix (replaces the hardcoded single-node build)

For the target node, walk `chorus:partOf` up to the root (a node with no partOf),
collecting every node. For EACH node in the chain:

- **ScopeKind** := derive from `rdf:type` (NOT hardcoded). Map:
  Role→Role(6) Service→Service(5) Domain→Domain(4) Product→Product(3)
  ValueStreamStep→Step(2) ValueStream→Stream(1). SELECTIVE: a node may be
  multi-typed (role-silas is `a Role` AND `a AgentRole`) — derive from the
  scope-bearing type, ignore the rest.
- **Graph** := resolve per-node, model-driven — `rdf:type` → that class's shape →
  `chorus:instancesGraph`, via the SAME `resolve_instances_graph` the serve uses.
  The graph is a property of THE NODE, not the route. Table stays dispatch-only.
- **Properties** := fetch that node's `hasProperty` rows FROM ITS OWN resolved graph.

Then pass the ordered ScopeNode chain to `decide_effective_value` (already built —
picks highest rank() that SETS the key).

### Hard constraint (Wren)

**Do NOT wildcard `GRAPH ?g`.** That would find a stale copy in a graph nothing
serves and answer confidently from it — the exact failure the per-node pin prevents.
Each node's graph is resolved and pinned, never searched.

## Negative proof (#3734) — required before this gates anything

- Three reachable, non-collapsible states (Wren keeps these in the handler, no wrapper):
  200 SET / 404 UNSET (chain built, key absent everywhere) / 502 UNREACHABLE.
  Current handler returns `{"error":"no property sets key"}` — cannot distinguish
  UNSET from UNREACHABLE (fuseki-down is also an error object). Fix as part of this.
- Precedence proof over HTTP: with the team default (100, global) on the stream and
  a role override at a DIFFERENT value (Jeff sets the number), /effective for
  role-silas returns the override (Role rank 6 beats Stream rank 1); for a role with
  no override, returns 100 walked from the stream. A role NOT anchored partOf returns
  unset. Each state reachable, none collapsible.

## Also open (separate, do not conflate)

- serve-prove check (`platform/tests/athena-serve-prove.sh`, this werk): built + runs,
  3 states, caught /effective red-before/green-after #3845. Its "claimed" set is too
  broad (counts functional domains like /search, /time). Narrow "claimed" to
  model-declared collection surfaces before trusting its 31/40 number.
- #3846: assemble athena-model → athena-make → athena-deploy as an act-run pipeline
  with this serve-prove as the proving step. The verbs exist; this composes them.
