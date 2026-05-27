# Crawler as Borg's 13th domain

**Silas · 2026-05-27 · #3106** · synthesis of Wren's crawler-decomposition design (#3069/#3071/asis-tobe.svg) into the Borg domain set.

## The move

Borg owned twelve domains: **builds, deploys, logs, properties, alerts-monitors, security-trust, time, heralds, toolchain, infrastructure, analytics, metrics**. With Wren's #3069 retiring the crawler monolith into a registry of declared instances, **crawler becomes the 13th** — and the most structurally consequential one, because it's the template the other twelve will inherit from.

## Why this is the template, not just another domain

The other twelve Borg domains are mostly bash convention + scattered scripts. Each new alert, probe, or herald adds another tendril without a contract. The crawler decomposition (#3069 AC2) introduces the first **instance contract** in Chorus:

```
{ name, source, target, cadence, costTier, mode, domains, write }
```

This is the move from `aether / humours / substrate` vocabulary (medieval) to declared instances with named edges (modern). Every new Borg domain instance gets the same contract: own cadence, right home, pull-not-push.

The structural argument from #3069 — "monolith couples cadence, write-home, and read-discipline" — applies one-for-one to the other twelve:

- **alerts-monitors:** alert rules today are a bash directory; should be declared instances (source = pulse/loki/spine, target = nudge/page/log, cadence = on-fire, costTier = cheap).
- **heralds:** the 2-heralds decomposition (below) is exactly this contract applied.
- **metrics:** Prometheus scrape configs are already this shape — should be modeled the same way.
- **properties:** property scans (#2737/#2738) become instances declaring source=file-watch + target=graph + cadence=on-change.

Land the crawler registry first → the other twelve inherit the model.

## The 2-heralds decomposition (code-herald + test-herald)

The SVG shows "code / tests" as **one instance** (reads: git/fs, writes: graph/knowledge, cadence: frequent). My read: that's still coupled — code and tests have different write-homes and different consumers.

**Proposal: split into two heralds.**

| Instance | Reads | Writes (home) | Cadence | Consumer |
|---|---|---|---|---|
| **code-herald** | `platform/**`, `roles/**` (source files) | graph: `chorus:File`, `chorus:code` domain | file-watch / minInterval=60s | code-search, blast-radius, gate-arch |
| **test-herald** | `**/*.test.ts`, `tests/**/*.bats`, test-run output | graph: `chorus:tests` domain + test-result events | post-test-run (push from runner) | gate-code, gate-quality, test-trend dashboards |

Why split:

1. **Write-home differs.** Code declarations live in `chorus:code`. Test outcomes (pass/fail/duration) live in a separate `chorus:tests` domain with time-series shape, not file-shape.
2. **Cadence differs.** Code scans on file-watch (write happens, scan fires). Tests fire on `npm test` / `cargo test` / bats run completion — a different trigger.
3. **Consumer differs.** Code-herald feeds blast-radius and architectural review; test-herald feeds gates and trend dashboards. Mixing them means one cadence serves both badly.
4. **Cost differs.** Test-run ingestion includes test output parsing + stack-trace extraction — heavier than a file-scan.

Open question for Wren: is this split inside the crawler-engine (two instance declarations) or one tier up (test-herald lives in `heralds` Borg-domain, code-herald in `crawler-engine`)? My read: **both are crawler-instances** because both fit the source→target→cadence→cost contract. The `heralds` Borg-domain is the SET (governance), each herald is an instance.

### The herald family is open (extensible by construction)

Code-herald + test-herald are the **first two**, not the whole set. The point of the instance contract is that adding a herald = adding a declaration, not re-architecting. Concrete future heralds already in the pipeline:

| Future herald | Reads | Writes | Cadence | Motivating gap |
|---|---|---|---|---|
| **desktop-sweep-herald** | `~/Desktop/*.svg,*.html,*.md` | doc-catalog | weekly | Wren's #3104 + Silas's #3106 both surfaced desktop-resident chorus diagrams; sweep-as-instance closes the class (her idea, captured here for the family). |
| **doc-catalog-drift-herald** | doc-catalog filePath entries | drift report → spine events | daily | The werk-pointing-filePath substrate gap (Kade's #3103, hit on 3 cards today): a drift herald checks every catalog entry's filePath resolves on canonical, surfaces stale entries. |
| **ontology-drift-herald** | Athena tree vs filesystem | spine events | on-change | Discovers when subdomain Owl declarations diverge from on-disk reality (today's reflexive grep-each-session pattern as instance). |
| **principle-stale-herald** | loom principle file mtimes | spine events | hourly | Existing class (#3009) folds into the family. |

**The family closes by construction.** Any new sense-and-emit pattern that fits `source → target → cadence → cost` lands as a herald declaration; no re-architecture of the synthesis note, no new Borg-subdomain, no new bash convention. That's the verb-as-substrate property applied to observability instances.

## How crawler-as-domain fits Borg's set

```
Borg (Silas)
├── builds
├── deploys
├── logs
├── properties
├── alerts-monitors
├── security-trust
├── time
├── heralds              ── 2-heralds decomposition lands here as instances
├── toolchain
├── infrastructure
├── analytics
├── metrics
└── crawler              ── NEW (13th) — registry + engine + N instances
    ├── code-herald      ── (instance)
    ├── test-herald      ── (instance)
    ├── cards-snapshot   ── (instance)
    ├── spine-index      ── (instance)
    ├── feedback-snapshot
    ├── ops-snapshot
    ├── ontology-sync
    ├── rdf-join         ── (the rare heavy one)
    ├── mentions         ── (pull-on-demand)
    └── logs             ── (pull-on-demand, Loki IS the store)
```

Ownership clarification: the **crawler-engine** is Borg (Silas). Individual **instances** may be co-owned by domain owners (e.g., test-herald might be Kade's test-infrastructure work flowing through Silas's engine). The Borg-domain governs the SET; instances declare into it.

## What this closes

- **The storm** (continuous 1.7–15s eventloop blocks, ~2–6/min all day) — structurally impossible once heavy instances run rarely.
- **6-cards-per-facet pattern** — recurring crawler work becomes a declaration update, not a fresh card per facet.
- **GET-heavy / PUT-thin / use-thin tell** — `mode: pull-on-demand` for mentions/logs stops the speculative-read waste.
- **"Aether / substrate" vocabulary** — instances have names, edges, owners. No more placeholder talk.
- **The invisible-background-block gap** — named instances are observable; a slow one is identifiable by name in the spine.

## What it does NOT close (out of scope here)

- **#3082 emission-off-loop** — the serving loop still emits pulse/andon/alert/eventloop synchronously. Worker-class split is separate work.
- **Other twelve Borg domains** — none of them get the instance contract just by landing crawler-domain. Each domain still needs its own migration card.
- **2-heralds the proposal itself** — needs Wren ACK on whether it lives inside crawler-engine or as separate Borg-domain instances.

## Related

- [crawler-instance-model.html](../../../designing/docs/crawler-instance-model.html) — the contract (#3069 AC2)
- [crawler-dependency-map.html](../../../designing/docs/crawler-dependency-map.html) — verified writers→outputs→consumers (#3071)
- [crawler-asis-tobe.md](../../../designing/docs/crawler-asis-tobe.md) — the lens (Wren, 2026-05-24)
- [system-architecture.md](../system-architecture.md) — Silas's living system view (this note linked from there)
