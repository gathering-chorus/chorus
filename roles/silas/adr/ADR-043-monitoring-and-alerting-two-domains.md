# ADR-043: Monitoring & Alerting — Two Domains, Six Monitor Types, a Push/Pull Contract

**Date:** 2026-06-13
**Status:** Accepted
**Decider:** Jeff (directive), Silas (architecture, ops/observability owner — DEC-022)
**Card:** #3411
**References:** #3405 (false-positive deep-health probes) · #3406 (the 535MB unbounded-read freeze root) · #3407 (alert routing should be config/ownership, not code) · #3082 (external eventloop probe) · #3400 (watermarks direct-timing) · `principle-self-healing` · `principle-services-reliable-bounded-idempotent` · `principle-simplicity-is-strength` · `feedback_jeff_is_not_the_monitor`
**Influences:** Charity Majors (observability vs monitoring; symptom/SLO alerting; high-cardinality wide events) · Google SRE (alert on symptoms, SLO burn)

## Context

On 2026-06-13 our service monitoring failed in every way at once, and the failures were diagnostic of a structural problem, not isolated bugs:

- **chorus-api crash-looped ~122 times in a day for weeks** and nothing ever alerted "this service is in a crash loop." The root (#3406 — three unbounded `readFileSync`/`readFile` of the 535MB `chorus.log` on the event loop) was found by reading `.ips` crash files and direct-timing the running system, **not** by a monitor.
- **The deep-health probes were 3/3 false positives** (#3405): `clearing` checked at `:1551` while it serves on `:3470`; `vikunja` returned 127 from the probe's own PATH, not a vikunja outage; `standards-surface` looked at the wrong path. Probes that cry wolf train the team to ignore the signal — trust collapses to zero.
- **The event-loop alert fired every ~5 minutes all day saying "blocked 8000ms, op=probe-timeout, no cause inferred."** It told us *something* was wrong, for weeks, but never *what*.
- **Alerts marked `delivered` never surfaced** — the routing was right (#3407) but the last-mile injection didn't reach the owner's terminal.
- **The monitors themselves were failing** — `deep-health` (exit 1), `daily-signal-scan` (exit 1), `nudge-health`/`heartbeat` (exit 78) — and nothing watched the watchers.

The common thread: **monitoring and alerting are conflated, probe-shaped, cause-noisy, and unobservable.** Each bash probe both *measures* and *decides to nudge*, so a wrong-port probe mis-measures **and** mis-fires in one coupled unit. We alert on causes (25 standing warnings nobody acts on) instead of symptoms. And when something is slow we cannot ask *why* — we have alarms without the dimensionality to locate the fire.

This ADR sets the architecture for fixing it.

## Decision

### 1. Monitor and Alert are SEPARATE domains

**Monitor domain = observation.** Its only responsibility is to produce a **truthful, structured signal**. It never decides who to wake. A monitor either lets the thing **push** (self-emit) or a collector **pulls** (scrape / query / synthetic probe). Output lands on a stream: the **spine** for events, **Prometheus** for gauges (both already run on the box).

**Alert domain = evaluation + routing + delivery.** It consumes signals and owns: **rules** (threshold / absence-over-X / SLO-burn), **severity**, **routing to the owner** (derived from the ownership model — #3407), **reliable delivery**, and **dedup / throttle / escalation**.

This split is load-bearing for three reasons:

1. **Mechanism vs policy.** Measurement is stable mechanism; thresholds, severity, and routing are policy that changes constantly. Policy belongs in **config/data, not code** (#3407 generalized — rerouting an alert must never require a recompile + deploy).
2. **One signal, many rules.** Today every probe both-measures-and-nudges, so the same wrong-port bug breaks measurement *and* alerting together. Decoupled, one truthful signal feeds many independent alert rules, and a measurement bug can't masquerade as an outage.
3. **The alert domain can monitor the monitor domain.** A monitor that stops emitting is itself an **absence signal**. This is how #3406 hid — the watchers went dark and nothing watched them. Meta-monitoring falls out for free once the domains are separate.

In the ontology, `monitor` and `alert` are **distinct domains** (this feeds the coherent-model program — the per-service signal contracts and per-alert routing rules are exactly the kind of thing that should be modeled, then generated, not hand-written).

### 2. Six monitor types, each with a data contract

Monitors differ by **what they watch** and **how the data arrives**. The taxonomy:

| Type | Watches | Push / Pull | Emitted data (the contract) | Alert rule |
|---|---|---|---|---|
| **Liveness** | persistent daemons | **push** | `service.alive {service, pid, ts}` every N | absent for X turns → page owner |
| **Freshness** | scheduled jobs | **push on finish** | `job.ran {job, status, ts, duration}` | no run within expected window → page |
| **Saturation** | resource health | **pull** (scrape) | RSS, CPU, FD, disk, eventloop-lag gauges | trend/threshold (RSS climb → catch OOM *before* the crash) |
| **Latency (SLI)** | request-serving services | **push per work-unit** | wide event: `served {service, route, op, duration_ms, status, bytes, trace}` | symptom: p99 / error-rate / **SLO burn** |
| **Errors** | everything | **push** | error events + **crash/restart count** | rate / crash-loop (the 122-restart class) |
| **Synthetic** | end-to-end flows | **pull** (active probe) | does the real flow work? (nudge self-to-self, a real search query) | fail → page; **probe-misconfig ≠ service-down** |

A monitored thing typically wears several: a request-serving daemon emits **liveness** + **latency** + **errors** and is **pull**-scraped for **saturation**.

### 3. Push vs Pull is a principle, not a per-case guess

- **Push the things that can vanish.** Liveness and per-work-unit events: a dead service cannot be pulled, and *absence is the signal*. Dead-man's-switch monitoring **cannot false-positive on a wrong port/path** — a down service literally cannot emit — which structurally kills the #3405 class.
- **Pull the gauges that persist.** Resource metrics want regular sampling regardless of whether anyone asked; scrape them (Prometheus's model, node-exporter already running).
- **Synthetic is active-pull by nature** — you must drive the end-to-end transaction; keep these, but the probe must distinguish *probe-misconfigured* from *service-down* and validate its own target.

### 4. Alert on symptoms and SLOs, not causes (Majors / SRE)

- **Page only on what impacts a user or role** (latency, errors, a broken flow). Everything else is a **dashboard/query, not a page.** This retires the 25 standing "warnings" — those are not alerts, they are noise wearing the alert costume, and they are why real signal gets ignored.
- **Define an SLO per service**: name the SLI (a row above — latency/error-rate), set a target, **alert on burn**, not on raw thresholds. "What data must be emitted?" → the SLI is the contract.
- **Jeff is never the monitor** (`feedback_jeff_is_not_the_monitor`): alerts route to the owning role, actionable, reliably delivered. Jeff sees outcomes, never discovers a stall first.

### 5. Observability, not just monitoring (the #3406 lesson)

Monitoring catches **known-unknowns** — we knew to watch eventloop-lag, so we got "blocked 8000ms." Observability answers **unknown-unknowns** — "blocked *by what?*" We had the alarm and could not ask the question. The fix is **wide, high-cardinality structured events** (the latency row) that can be sliced after the fact by `{service, op, file, size, card, role, trace}` — not more bespoke probes. The **spine is already an event log** — the right substrate, provided events carry the dimensions and we do not pre-aggregate them away.

## Where this lands on our stack

We already run **Prometheus** (scrape/store gauges — saturation), **Alertmanager** (rules/routing/dedup — the alert domain's evaluation engine), **Loki** (logs), and the **spine** (event store — liveness, freshness, latency, errors). The chorus-side monitoring bypasses all of it with bespoke bash probes that both-check-and-nudge. The target architecture: **monitors emit to spine/Prometheus → one alert evaluator (Alertmanager-style rules + ownership routing) → reliable delivery via pulse.** We are not adding tools; we are using the ones we have correctly and deleting the bespoke probe-and-nudge scripts.

## Consequences

- **Liveness heartbeat is the first build** — it is one row of the table, and it is the exact gap that let #3406 hide for weeks. Every persistent daemon emits `service.alive` every N seconds; one absence-detector pages the owner after X consecutive misses. (Jeff's directive; its own card.)
- **The bespoke deep-health/probe scripts get retired** as their signals move to the typed contracts (#3405 is subsumed — wrong-port probes cease to exist because liveness is self-emitted).
- **Alert routing becomes config/ownership-derived** (#3407 generalized) — no code change to reroute.
- **Monitor/alert become two domains in the ontology**, and the per-service signal contracts + per-alert rules become **generated from the model**, not hand-maintained (coherent-model program).
- **Meta-monitoring is automatic** — the alert domain watches that every expected signal arrives; a dark monitor is an absence alert.
- **Scheduled jobs (≈39) need the freshness contract**, distinct from the daemon heartbeat — same alerting spine, different emit.
- **Any monitor/alert UI surface renders from the model/config and conforms to the #3415 design system (system.css) — never a bespoke one-off page** (Kade's conformance constraint). The alert cockpit, the monitor-state views, and the #3410 go-token cockpit project alert/monitor state *from* the typed contracts + ownership model; they do not hand-roll a new page each time. This is the same anti-sprawl / no-competing-implementations principle the chorus-out-of-gathering extraction (#3361) is currently unwinding — a monitoring UI that spawns bespoke pages would re-create exactly that debt. **UI-conformance dependency: #3415 (design system).**
- **The standards-surface re-home (#3361 residue) is a clean first consumer of the freshness/truthful-probe contract** — when the artifact lands in its chorus-api home, its probe target follows the artifact (the #3405 lesson made structural), demonstrating the "probe declares its target, target moves with the artifact" contract.

## Follow-on cards (priority order)

1. **Liveness heartbeat + absence-detector** (P1) — the #3406-class gap. `service.alive` emit lib + one detector + ownership-routed page.
2. **Probe truthfulness / retire bespoke probes** (#3405) — fold into the typed contracts.
3. **Latency SLI wide-events on request-serving services** — observability layer; symptom/SLO alerting.
4. **Scheduled-job freshness contract** for the ≈39 cron jobs.
5. **Alert domain as config-driven routing** (consume #3407's direction; generalize recipient = ownership lookup).

## Open questions for Jeff

- **N** (heartbeat interval — 30s?) and **X** (consecutive misses before page — 3 ≈ 90s dark?).
- Whether the alert evaluator is **Alertmanager** (reuse) or a **chorus-native evaluator over the spine** (more coherent with the model, more to build). Leaning: Alertmanager for gauge/SLO, spine-native absence-detector for liveness/freshness — two evaluators is fine, mixed *mechanisms* are not.
