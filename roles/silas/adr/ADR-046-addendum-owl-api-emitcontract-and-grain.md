# ADR-046 Addendum: owl-api as the reference normalized-emitter + the deployable-grain decision

**Status:** Proposed
**Date:** 2026-06-19
**Author:** Silas
**Card:** #3505 (spec, AC-3 + AC-4) · implemented by #3506 (Wren) · rolled-up by Borg
**Extends:** ADR-046 (EmitContract) · ADR-047 (response contract) · ADR-048 (security)

## Context

ADR-046 made the EmitContract a *declared* thing: a Service/Domain declares the Metrics it emits, bound to the spine events that carry them; Borg leverages. owl-api already emits `api.request.served` (RED dimensions) and `emit_write_spine` — so owl-api is the natural **reference normalized-emitter**: get its EmitContract right, and every generated surface emits the same shape, so Borg rolls up per-product/per-version with zero per-surface alert rules. This addendum specs that normalization (AC-3) and settles the deployable grain (AC-4).

The 2026-06-19 research pass flagged: a strong RED base but missing **saturation** (the 4th golden signal), **request-id/W3C-traceparent** (finding O1), OTel-conformant names, and PII redaction. Folded in below.

## AC-3 — The normalized owl-api EmitContract

### One canonical emit shape

Every generated surface emits the SAME dimensioned event — `api.request.served` for reads, `emit_write_spine` for writes — carrying:

```jsonc
{ "service":"owl-api", "class":"Domain", "entity":"chorus:properties", "route":"/v1/domains/properties",
  "fold":"read", "status":"200", "result_count":35,
  "total_ms":12, "upstream_ms":4,                         // RED: rate(count) / errors(status) / duration(ms)
  "caller":"silas", "trace_id":"00-<trace>-<span>-01",    // W3C traceparent (finding O1)
  "product":"athena", "apiVersion":"v1", "shapeVersion":"2026-06-19", "commit":"534805b9" }   // NEW dims
```

- **New dims** (the addendum's core): `product`, `apiVersion`, `shapeVersion`, `commit` — so Borg rolls up **per-product and per-version** without bespoke rules. `class`/`route`/`fold`/`status`/`*_ms`/`caller`/`trace_id` already exist in the crate (code review 2026-06-19).
- **Coverage = RED + write-health + liveness + saturation:**
  - **RED** — rate (event count), errors (status≠2xx), duration (`total_ms`) — present.
  - **Write-health** — `emit_write_spine` carries the validation outcome (422 rate = bad-input signal) + the auth allow/deny (ADR-048 §6).
  - **Saturation** (4th golden signal, research gap) — a periodic `owl-api.saturation` emit: in-flight requests, upstream (Fuseki) latency p95, queue depth.
  - **Liveness** — a heartbeat emit (and `/livez`+`/readyz` from ADR-047 §7) so "serving but wedged" is distinguishable from "down."
- **OTel-conformant metric names** (`http.server.request.duration`, etc.) so off-the-shelf tooling reads them — same conform-to-standards / interoperability lesson as the model's `rdfs:`/`sh:` vocab.
- **PII/secret redaction** (research gap): `caller`/`entity` are IRIs/roles, never credentials; no token/PII values in any emit (the write_scrubber boundary, on the emit side).

### Correlation + traces as the join key (finding O1)

One trace id threads everything: owl-api **accepts/generates W3C `traceparent`**, **propagates** it downstream to Fuseki (so `upstream_ms` attributes to the same trace), **echoes** `X-Request-Id`/`traceparent` to the caller (ADR-047 §6), and stamps `trace_id` on **every** emit line. This is NOT a new trace system — it bridges owl-api's existing `effective_trace` + the existing chorus `traces` table/reader (#2101/#3163) to the W3C standard. Result: logs ↔ metrics ↔ traces join on `trace_id`, and the trace spans caller → owl-api → Fuseki + owl-api → spine → Borg. The instrument exists; this conforms + wires it (no parallel build — the stranded-instrument lesson).

### Borg's roll-up

Because the shape is canonical + dimensioned, Borg computes RED/saturation per `(product, apiVersion, shapeVersion)` from the one stream — no per-class alert rules (the drift-sprawl we're killing). SLO thresholds are config Properties (ADR-044/045: thresholds-as-properties = SLO-as-code), not hardcoded.

## AC-4 — Deployable-grain decision

**Decision: keep ONE PID now; SPEC (don't build) per-product isolation as the future cut.**

- **Now:** owl-api is one binary, one PID (`com.chorus.owl-api`, #3466), serving every primitive + product + future domain on ONE origin (`:3360`). build/deploy = **`werk-build` / `werk-deploy` as-is** — no new deploy machinery (Jeff: "we use werk-deploy and werk-build, don't do something different"). The model deploys via `chorus-model-deploy.sh` wired into the pipeline deploy step (#3509's deployer; #3499 one-pipeline integration, Kade co-owns).
- **Product wiring:** products (chorus/loom/athena) compose via the generated product-index (`project_product_index`, already in the crate), NOT separate PIDs. One origin, many surfaces.
- **Future isolation cut (SPEC, deferred):** if a product needs independent scaling/blast-radius, `owl-api serve --product X` on its own PID is the cut-line — the binary already takes `--class`/`--product`, so it's a launchctl-plist change, not a code change. **Don't build it until a real need forces it** (spike-before-building; zero-instance speculation is the phantom trap).

Rationale: one PID is the simplest thing that works (Gall's Law — matches #3466), it's observable as one unit, and the grain can split later without a rewrite because the binary is already parameterized. Splitting now would be speculative complexity.

## Consequences

- Borg gets per-product/per-version observability for free off one canonical stream.
- Saturation + liveness + traces close the research gaps → the emit side is open-source-grade like the contract.
- Deploy stays on the proven werk-build/werk-deploy path; grain can evolve without a rewrite.

## References

- ADR-046 (EmitContract) · ADR-047 (response contract) · ADR-048 (security) · ADR-044/045 (thresholds-as-properties) · #3466 (one-origin multi-class)
- W3C Trace Context · OpenTelemetry semantic conventions · Google SRE 4 golden signals · research pass 2026-06-19
- code review 2026-06-19: `api.request.served`/`emit_write_spine`/`effective_trace`/`project_product_index` already in the crate
