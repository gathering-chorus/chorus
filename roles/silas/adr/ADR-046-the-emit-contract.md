# ADR-046 — The Emit Contract: domains declare what they emit; Borg leverages

- **Status:** Proposed (2026-06-18)
- **Owners:** Silas (model/contract), Kade (werk emitter — #3495)
- **Relates to:** ADR-044 (PropertyKey governance), ADR-045 (domain-is-an-owl-class), #3489 (properties domain), #3494 (owl-api vocab fan-out), #3495 (werk emitter)

## Context

Borg is the reflection layer — the system seeing itself. Today observability is assembled *outward-in*: dashboards and queries reach into whatever events happen to exist and reconstruct meaning after the fact. That inverts the dependency. The thing being observed knows best what it emits; the observer should read a **declaration**, not reverse-engineer the stream.

Two failure modes this produces, both seen this week:
- **Absence leaks.** A service that stops emitting looks identical to a healthy-but-quiet one. The overnight false-alarm barrage and the chorus-api freeze both hid in absence.
- **Dishonest metrics.** Change-failure-rate computed over *all* `werk.land.failed` events counts tooling bugs (pr-already-exists, branch-mismatch) as change failures, poisoning the signal.

## Decision

**A Service or Domain `emits` an `EmitContract` — a declared, governed set of `Metric`s, each computed from the real spine events it binds to.** owl-api projects the contract into monitors (the 7th generated surface). The contract is **data**, not code.

**Conform-don't-invent** — the vocabulary is the union of three established standards, not a bespoke scheme:
- **OTel semantic conventions** → *declare-what-you-emit*. `chorus:emits` / `EmitContract` is the declaration.
- **RED / Golden / USE** → metric primitives. `chorus:metricKind` ∈ {`rate`,`error`,`duration`,`saturation`,`utilization`}.
- **DORA** → werk is our CI/CD, so its emissions ARE the four keys: `dora.deployFreq`, `dora.leadTime`, `dora.cfr`, `dora.mttr`. Plus `liveness`.

Three load-bearing design moves:

1. **Metrics are computed from spine events, never dual-authored.** `chorus:emittedAs` binds each metric to the real event name(s) Kade verified live (`card.pulled`, `demo.presented`, `go`, `card.accepted`, `werk.land.failed`, `merge.refused`, `werk.heartbeat`). No separate metric store to drift.

2. **Lead-time decomposes.** `dora.leadTime` is three legs via `subSpanOf`: build (`card.pulled→demo.presented`), **coordination-wait** (`demo.presented→go`), land (`go→card.accepted`). Round-churn surfaces as coordination-wait, not pipeline time — the distinction that makes the metric actionable.

3. **`failureClass` is governed and emitted at the source.** A registered `PropertyKey` (`pk-failureClass`, enum `change`|`tooling`) — CFR counts only `change`; `tooling` is excluded so today's land.failed bugs never poison it. Emitted *at the refusal point* in werk (#3495), not post-hoc classified → CFR honest **by construction**.

4. **Thresholds are properties.** A metric's SLO bound is a config `Property` via `chorus:hasThreshold` — SLO-as-code. Changing an SLO is a model write. Ties ADR-046 to the properties domain (#3489).

## Model (live in `chorus.ttl`)

Classes: `EmitContract`, `Metric`. Edges: `emits` (Service|Domain→EmitContract), `declaresMetric`, `subSpanOf`, `hasThreshold` (Metric→Property). Data: `metricKind`, `emittedAs`, `spanStartEvent`, `spanEndEvent`. Shapes: `EmitContractShape` (≥1 metric), `MetricShape` (metricKind + emittedAs required). Governance: `pk-failureClass`. Worked proof: `chorus:werk-emit-contract` declaring the six werk metrics — round-trips committed→Fuseki→served.

## Consequences

- **+** Observability becomes a generated surface, not hand-assembled. A new domain gets monitors by declaring its contract.
- **+** Absence is representable: a `liveness` metric's missing emission is the unhealthy state.
- **+** CFR/lead-time are honest by construction (source-emitted discriminator, decomposed span).
- **−** Requires every domain to author a contract to be observed. Acceptable: it's the same declare-what-you-emit discipline OTel already asks for, and the contract is small.
- **Open:** `mttr` needs an incident open/close event pair on the spine (not yet emitted). `deployFreq` binds to deploy events — confirm names. Both follow-ons, not blockers.

## Sequence

The OWL vocabulary lands with #3489 (this commit). Kade authors werk's concrete contract + the source-side `failureClass` emission in #3495 against `chorus:werk-emit-contract` as the template. owl-api (#3494) projects EmitContract into the monitors surface once the vocab fan-out generalizes past CRUD.
