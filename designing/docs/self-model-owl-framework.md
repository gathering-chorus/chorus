# Athena v2 — the driver for the rework

**2026-06-02 · Wren + Jeff (+ Silas via #3185).** Synthesis of a long architecture thread, corrected after Jeff clarified what Athena v2 actually is. Captures a direction; does not yet commit code beyond the core-spine sketch.

## Thesis

The system holds **one model of itself**, and that model is **Athena v2** — the clean redesign in `data/athena/tree.json`. **v2 is the driver.** From v2 we *generate* the rest of the surface — the OWL, the SHACL shapes, the API, the MCP tools, the TS/Rust types — so the surface is **defined by v2** and divergence is structurally impossible because everything descends from one source.

The legacy RDF graph (Fuseki, `chorus.ttl` / `borg.ttl`) is **v1** — what we migrate *from*, not the source we build on.

> Correction from earlier today: I briefly argued the bigger RDF graph (49 nodes) was "the real model." Wrong — **bigger ≠ newer.** v2 is defined by being the *clean rebuild*, not by row count.

## The two Athenas (why v2 is the driver, even though it's smaller)

| | **v2 — the driver** | **v1 — the legacy** |
|---|---|---|
| Where | `tree.json` (JSON) · `/api/athena/tree` | Fuseki / RDF · `/api/athena/subdomains` |
| Size | 33 domains | 49 nodes |
| Shape | clean: one namespace, consistent names, value-stream-aligned, the 4 axes, today's ownership | messy: 3 namespaces bridged by `owl:equivalentClass`, drift (Proving catch-all, `loom-rcas` twice, services mistyped as subdomains) |
| Coverage | **Chorus stream only** — Gathering not migrated yet | **complete** — includes the Gathering collections (photos, music, books, seeds, stories, people…) |
| Role | the redesign / the destination | the organically-grown past |

v2 is *smaller* only because the **v1 → v2 migration is partway**: the Chorus side has been rebuilt cleanly into v2; the Gathering collections still live only in v1. Their absence from v2 is "not migrated yet," not "incomplete baseline."

**So the rework has two halves:**
1. **Migrate v1's content into v2's clean structure** — first and foremost the **Gathering** stream, which exists *only* in v1 today.
2. **Generate the surface from v2** — OWL + SHACL + API + MCP + types, so v2 is the single source and the API can't drift from the model.

## The flow

<pre class="mermaid">
graph LR
  V1["v1 — Fuseki/RDF (49)<br/>legacy, incl. Gathering"] -.migrate into.-> V2
  HARV["facet-heralds (#3185)<br/>discover-*"] -.instance data.-> V2
  V2["<b>Athena v2 — tree.json</b><br/>the clean model · the SOURCE"] -->|generate| GEN["OWL · SHACL · API<br/>+ logging · security · MCP · types"]
  GEN --> C["consumers:<br/>context-inject · ownership · werk-tests"]
  classDef src fill:#eafaf1,stroke:#1e7045,stroke-width:2px,color:#11161b;
  classDef old fill:#fdecea,stroke:#c0392b,color:#641e16;
  class V2 src; class V1 old;
</pre>

## OWL's role (corrected)

Jeff's earlier phrase was "the OWL defines the API." That still holds — **but the OWL is downstream of v2, not the hand-authored source.** v2 (JSON) is authored/migrated; the OWL + SHACL are *generated* from it (LinkML-style); the generated OWL is the API contract. The legacy `chorus.ttl` / `borg.ttl` aren't the source either — their content gets reconciled into v2's vocabulary, then the OWL is regenerated. One source (v2), many generated surfaces — that's what makes impedance impossible.

## Tooling (verify, don't assume)

- **LinkML** — a schema (YAML/JSON) as single source → generates **OWL + SHACL + JSON-Schema + Pydantic/TS/Rust**. This fits **v2-as-JSON-source** directly: model in v2's form, generate everything else (incl. our types). Best match.
- **OBA** — OWL → OpenAPI + SPARQL-backed REST server. Fits only if **OWL is the source**; since v2 is JSON, OBA would sit *downstream* of the generated OWL, not at the source.
- Both unverified against our exact stack / 2026 maintenance — a half-day spike, not a given.

## What exists (build on, don't reinvent)

| Piece | Where | State vs v2 |
|---|---|---|
| **Athena v2** | `data/athena/tree.json` (33) | **The driver.** Clean, Chorus-only, JSON. Gathering pending migration. |
| Legacy graph | `chorus.ttl` v0.3.0 (3499 lines) + `borg.ttl` + `framework.ttl` | **v1.** Has the spine + facet classes (`CodeFile`, `LogSource`, `Engine`, `Monitor`…) and the Gathering data (`jb:`), but messy/divergent. Migrate the *content* into v2; regenerate OWL from v2. |
| Borg module (designed) | `borg-service-design.md` (2026-04-15) | The borg classes are specced; they become v2 facet/service classes once migrated. |
| Harvest layer | `silas-3185/.../harvest-layer.html` (#3185) | Feeds v2's *instance* data (facet-heralds → the model). Silas's build. |
| ADRs | ADR-025/021/020/028 | ADR-028 (zod canonical → SDK+HTTP+MCP derive) is the closest existing "one source → many surfaces." |

**Existing cards this touches (advance, don't re-file):** #1771 (reconcile ontology → answer: v2 is canonical, generate the rest), #2092 (declare products), #2676 (single-source derive), #3143 (codegen precedent), #3184 (semantic tagging — *this werk*), #3185 (harvest = instance feed).

## What's genuinely missing (the new thing)

The **generator**: nothing today turns the v2 model into OWL + API + types. `framework.ttl` is a hand-built bridge; #3143 is one narrow schema. The wedge is a v2 → (OWL/SHACL/API/MCP/types) generator, proven on one facet.

## The composed model (generated from v2)

<pre class="mermaid">
graph TD
  v2["<b>Athena v2 (tree.json)</b> — source of truth"] -->|generates| core["chorus-core<br/>spine + 4 axes + base SHACL"]
  core --> loom["loom · Wren"]
  core --> ath["athena · Wren"]
  core --> clr["clearing · Wren"]
  core --> wk["werk · Kade"]
  core --> bg["borg · Silas<br/>(+ harvest facets)"]
  core --> cv["convergence · Kade"]
  classDef s fill:#eafaf1,stroke:#1e7045,stroke-width:2px;
  class v2 s;
</pre>

**Make-or-break rule: products EXTEND the core, never REDEFINE it.** One definition of `Domain` and the four axes; products only add their own classes. (Whether the modules are authored in v2's form and the OWL generated, or authored as OWL — see open questions.)

### The products ARE the value stream

Each value-stream step is realized by its product(s): **Shaping** = Loom + Chorus (Wren) · **Designing** = Athena (Wren) · **Directing** = Clearing (Wren) · **Building** = Werk (Kade) · **Proving** = Borg (Silas) · **Operating** (cross-cutting) = Convergence (Kade). Order (Jeff-ruled): Shaping → Designing → Directing → Building → Proving.

### Four axes on every entity

`inDomain` (subject) · `ownedBy` (implementer — agent role only; **never Jeff**, who `headsProduct` over the whole) · `atStep` (value-stream step) · `shape` (skill-only | skill+hook | hook-only | verb | facet). #3184 tags the semantic instances (skills/hooks/heralds); building/proving facets self-tag (facet = domain).

### Recursive (fractal)

A step can host its own value stream: **werk-v2 IS the Building step's stream** (pull→commit→push→build→deploy→accept→demo). Zoom into any step → its substream.

### Bundled per service

Declare a domain in v2 → generate **API + logging + security + MCP** uniformly (kills the per-service hand-wiring: services that log nothing, hand-written MCP thin-skin, uneven scrubbing). Provider-agnostic: the model knows "service with port + health"; the herald carries the impl.

## #3177 — stands

The cut of `chorus_subdomains_list/get` (the v1/49 MCP path) is **correct for the Chorus scope we're reworking**: v2 holds the cleaner Chorus ownership, and restoring v1 would only re-add the divergence #3177 removed (and expose v1's wrong ownership). The one thing v1 still has that v2 lacks is **Gathering** ownership — **explicitly out of scope right now.** Not a reason to revisit the cut.

## Rollout (anti-big-bang) — Chorus scope

1. **Prove the generator on one facet** in chorus, no NiFi — v2 → API for one domain (e.g. `tests`). Bounded; touches no live API.
2. **Borg/harvest** (#3185) feeds v2 instance data → rep proving the loop.
3. **#3184** finishes the semantic tags.
4. **Migrate the variant live APIs** (athena handlers like `athena-subdomains.ts`) onto the generated path, one at a time.

*Gathering migration — deferred; not part of the current rework.* **Convergence (NiFi)** is the likely eventual harvest substrate but heavy + stranded in gathering — not the current bet. Prove simply in chorus first.

## Ownership

Core/v2 model — Wren (with Silas, who holds the legacy `chorus.ttl`). Per-product modules — loom/athena/clearing = Wren, werk/convergence = Kade, borg = Silas. The generator — likely Kade (build/codegen) on the v2 contract from Wren. werk-tests — Kade (#3185 consumer).

## Open questions (for Jeff — not assumed)

1. **v2's canonical *form*:** stays JSON (`tree.json`) with OWL generated from it (LinkML path)? Or do we move the authored form to OWL? My `chorus-core.ttl` is a *sketch of the core's shape* — if v2 (JSON) drives, the core is authored in v2's form and the OWL is generated, not hand-written TTL.
2. *(Gathering migration — parked; not a current concern.)*

## Related
- `athena-49-subdomains.html` — v1 (the legacy 49, both streams) · `athena-33-treejson.html` — v2 (the driver, Chorus-only)
- `athena-borg-conceptual-model.html` — the v1 drift we migrate away from
- `borg-service-design.md` · `silas-3185/.../harvest-layer.html` (#3185) · `chorus-core.ttl` (core sketch)
- ADR-025 / ADR-028
