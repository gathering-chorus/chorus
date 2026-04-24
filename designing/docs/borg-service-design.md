# Borg — Service Design

**2026-04-15 — Silas + Jeff.** Replaces observability-only service design (#2273). Borg is the reflection product. Incorporates prior art from Good Borg (Wren), Borg Mitosis (Wren), Assimilation Pattern (Silas), Codebase Decomposition Spike (Silas), Emergent Evolutionary Architecture (Wren).

The system sees itself. Borg reflects on everything it can reach — its own code, its infrastructure, its health, its security posture. Heralds discover structure. The graph assimilates it. The result is a system that knows what it is, what it runs on, what's watching it, and where it's blind.

**Reflection, not extraction.** The system looks at itself and recognizes its own structure. The model stays provider-agnostic — the graph knows "service with port and health check," not "LaunchAgent with plist path." Implementation details belong to the herald, not the model.

**Engine → Environment → Resource.** Three layers of the same thing at different granularity. An engine (toolchain) is what it is — Fuseki 5.1.0. An environment (infrastructure) is where it runs — fuseki-pods on Library, 2G heap, port 3030. A resource is what's inside — `urn:chorus:ontology`, a named graph in that environment. Content domains see resources ("seeds uses this graph"). Toolchain sees engines ("we run Fuseki"). Infrastructure sees environments ("this instance runs on this host"). Three views into the same graph.

## Product Position

Three peer products in the Gathering ecosystem:

| Product | Purpose | Domains | Builder |
|---|---|---|---|
| Gathering | Content — Jeff's collections and creations | Music, Photos, Books, Seeds, Notes, Ideas, ... | Kade |
| Chorus | Coordination — team workflow and communication | Cards, Gates, Spine, Clearing, Skills, Roles | All roles |
| Borg | Reflection — the system sees itself | Toolchain, Infrastructure, Commits, Deploys, Alerts & Monitors + cross-cutting: Observability, Security | Silas |

## Borg Domain Architecture

Seven domains. Five are layers with dependencies. Two are cross-cutting capabilities that span all layers.

| Domain | Type | What it contains | Depends on |
|---|---|---|---|
| Toolchain | LAYER | Engines the system uses: Fuseki, Node, Docker, Prometheus, MySQL, NiFi, Ollama, etc. Versions, configs, known issues. | — |
| Infrastructure | LAYER | Where engines run. Three surfaces: compute (services, processes), storage (databases, volumes), network (ports, tunnels, routes). | Toolchain |
| Commits | LAYER | How code gets versioned. git-queue.sh, pre-commit hooks, WIP gate, write-scrubber, commit lock, push/rebase flow. | Toolchain, Infrastructure |
| Deploys | LAYER | How code gets running. app-state.sh, bind mounts vs full deploys, rollback paths, freeze rules, deploy targets. | Infrastructure, Commits |
| Alerts & Monitors | LAYER | What's watched and what fires. Prometheus rules, deep-health checks, Promtail streams, blackbox probes. Coverage scoring against dependency graph. | Infrastructure |
| Observability | CROSS-CUTTING | Can we see what's happening? Aggregated health lens across all layers. Metrics, logs, dashboards, instrumentation. The capability that prevents breaking at speed. | All layers |
| Security | CROSS-CUTTING | Can someone else break it? Auth, ports, CVEs, permission models, write-scrubber, session boundaries. Coverage scoring for risk. | All layers |

**Dependency flow:** Toolchain → Infrastructure → Commits → Deploys → Running system. The application (Gathering, Chorus) depends on Infrastructure and Toolchain. Alerts & Monitors depend on Infrastructure. Observability and Security watch everything.

## Entity Model: Engine → Environment → Resource

Every technology in the system exists at three levels of granularity. The same entity appears on different domain pages from different perspectives.

| Layer | What it is | Example | Shown on |
|---|---|---|---|
| Engine | The technology itself — name, version, type, config language, known issues | Apache Jena Fuseki 5.1.0 | Toolchain domain page |
| Environment | A running instance — host, port, heap, config, health endpoint | fuseki-pods on Library, :3030, 2G heap, 13GB TDB2 | Infrastructure domain page |
| Resource | What's inside the environment — databases, graphs, tables, collections | `urn:chorus:ontology` (named graph, 520 classes) | Content domain page (Persistence section) |

**How domain pages use this:**

| Domain page | Persistence/Instances section shows |
|---|---|
| Seeds (content domain) | Resources this domain uses: `urn:jb:seeds` graph (Fuseki env), messages table (SQLite env), embeddings (LanceDB env) |
| Toolchain (borg domain) | All engines and their environments: Fuseki → fuseki-pods (Library), MySQL → wordpress-db (Library) + vikunja-db (Library), MongoDB → images-db (Bedroom) |
| Infrastructure (borg domain) | All environments on each host: Library runs fuseki-pods, gathering-app, chorus-api, ... Bedroom runs images-api, ollama, navidrome, ... |

**Same data, three views.** A single Fuseki instance appears as an engine on the toolchain page, as an environment on the infrastructure page, and as resources on content domain pages. The graph connects them. The herald discovers each layer.

## Toolchain LAYER

The engines the system is built on. Each has its own configuration language, health model, upgrade path, and failure modes. An engine may have multiple environments (instances) across hosts.

**Engines (current):** Apache Jena Fuseki 5.1.0 (SPARQL/RDF), Node.js (application runtime), Docker (container runtime), Prometheus (metrics), Loki (logs), Grafana (dashboards), MySQL (relational), MongoDB (document), SQLite (embedded), LanceDB (vector), Apache NiFi 2.8.0 (pipelines), Ollama (LLM inference), Rust/Cargo (hooks shim), Navidrome (music), WordPress/PHP (blog), Cloudflare (tunnel/DNS).

**Environments (examples):**
- Fuseki → fuseki-pods (Library :3030, 2G heap)
- MySQL → wordpress-db (Library :3306), vikunja-db (Library :3306)
- Node.js → gathering-app (Library :3000), chorus-api (Library :3340), clearing (Library :3470), messaging (Library :3475), images-api (Bedroom :3001)
- MongoDB → images-db (Bedroom :27017)
- Ollama → ollama (Library :11434), ollama (Bedroom :11434)
- NiFi → nifi (Bedroom :8443)
- Navidrome → navidrome (Bedroom :4533)

**Herald: discover-toolchain**
- Scan: package.json, Cargo.toml, docker images, plist ProgramArguments, installed binaries
- Output: `borg:Engine` with name, version, type, config location
- Links to: `borg:Environment` instances via `borg:hasEnvironment`
- Coverage: what engines are tracked vs untracked

**Domain page sections:** Not the same as content domains. Toolchain needs Engines (catalog), Environments (instances per engine), Versions (upgrade status), Known Issues, Dependencies (engine-to-engine), Config Locations.

## Infrastructure LAYER

Where engines run. Today: home cloud (two Macs, Docker, LaunchAgents, LAN). Tomorrow could be any provider. Three surfaces: compute, storage, network. The infrastructure domain shows environments grouped by host.

**Compute:**
- Library: 15 always-on services, 9 observability agents, 30+ scheduled jobs
- Bedroom: 6 services, NiFi pipeline, 2 observability agents
- 1 Docker container (CSS)
- Each service is a `borg:Environment` of an engine

**Storage:** Resources inside environments — Fuseki 3 named graphs, MySQL 2 databases, SQLite 2 databases, LanceDB 1 collection, MongoDB 1 database, Logs 3.7 GB across `~/Library/Logs/`.

**Network:** Library 28+ listening ports, Bedroom 13+ listening ports, LAN Library↔Bedroom (0.68ms), Cloudflare tunnel (inbound seeds), SSH tunnel Bedroom :3100 → Library :3102 (Loki).

**Heralds:**
- `discover-services` — scan plists, docker-compose → `borg:Environment`
- `discover-storage` — scan databases, volumes → `borg:Resource`
- `discover-network` — scan ports, tunnels → `borg:NetworkEndpoint`
- Model is provider-agnostic: no plistPath, no dockerImage

## Commits LAYER

How code gets from written to versioned. The machinery between editing a file and having it on main.

- `git-queue.sh` — serialized commits with lock file
- Pre-commit hooks: WIP gate, write-scrubber, TypeScript checks
- Commit lock: prevents concurrent commits across roles
- Push/rebase: dirty-tree handling, atomic stash+rebase+push

**Herald: discover-commit-pipeline** — scan settings.json hooks, pre-commit config, git-queue.sh → pipeline stages, gate points, hook modules. Coverage: what hooks run, what they check, what bypasses exist.

## Deploys LAYER

How code gets from versioned to running. Each service has a deploy path with different characteristics.

- `app-state.sh` — service lifecycle management
- Bind mounts: views/CSS instant (no deploy needed)
- Full deploy: TypeScript changes need `app-state.sh deploy`
- LaunchAgent: `launchctl kickstart`
- Docker: container restart
- Rollback: git revert + redeploy

**Herald: discover-deploy-paths** — scan app-state.sh targets, bind mount configs, deploy scripts → deploy targets, pipeline type, rollback path, freeze rules. Coverage: what services have deploy paths vs manual-only.

## Alerts & Monitors LAYER

What's being watched and what triggers when it breaks. Alert coverage is to failure modes what test coverage is to code paths.

**Monitors (what watches):** Prometheus scrape targets (15s interval), deep-health.sh checks (5min, 13+ checks), Promtail log streams, Blackbox probes (HTTP endpoint checks), Node exporter (host metrics, both machines).

**Alerts (what fires):** Prometheus rules (7 files in shared-observability/), shell rules (`proving/domains/alerts/`), Grafana alerts (3 groups), deep-health nudge (failure → nudge Silas).

**Coverage model:**
- For any service: upward dependencies (who breaks if I die?) + downward dependencies (what do I depend on?)
- Score: dependency edges with monitors / total edges
- Gap = uncovered dependency = card waiting to be written
- Pyramid: unit monitors (single service), integration (dependency chain), E2E (smoke checks)

**Herald: discover-alerts** — scan Prometheus rule files, alerting/ YAML, deep-health.sh checks, Promtail config → monitors mapped to services, alert rules mapped to monitors, coverage score per domain. **Key output: what's NOT monitored — the blind spots.**

## Observability CROSS-CUTTING

The capability that prevents breaking at speed. Not a section ON a domain page — a lens ACROSS domain pages. Every domain shows its own health; observability is the aggregated view.

- **Metrics pipeline:** Prometheus (:9090) ← node-exporter, blackbox, mysqld-exporter. 15-day retention, 15s scrape interval. Grafana dashboards: 13 deployed.
- **Logs pipeline:** Loki (:3102) ← Promtail (both machines). 7-day retention, JSON format required. Bedroom ships via SSH tunnel to Library Loki.
- **Health pipeline:** deep-health.sh (5min) → `/tmp/deep-health-latest.json`. Pulse (per-prompt) assembles team state. Session-start reads pulse → role opening.
- **Instrumentation:** Hooks pulse log (62K+ lines, 14 modules), spine events (chorus.log), standards surface (generated compliance report).

## Security CROSS-CUTTING

Can someone else break it? Same coverage model as alerts — score exposed surfaces against secured surfaces.

**Current controls:** write-scrubber hook (blocks credentials in shared files), CSRF validation on app, Twilio signature verification, session auth on localhost:3000 (browser only), no auth on API endpoints (localhost trust model), SSH key-only for Bedroom.

**Herald: discover-security** — scan open ports (0.0.0.0 vs 127.0.0.1), auth config, CVE databases for engine versions, hook permissions → exposure map per service, auth coverage, version risk. Coverage: secured surfaces / total exposed surfaces.

## Herald Pattern

Every domain is reflectable via the same pattern. Kade proved it on the application surface (code, tests, pages, endpoints). The same architecture extends to borg's domains.

| Herald | Scans | Writes | Entity type |
|---|---|---|---|
| discover-code | Source files by domain | Code inventory per domain | `chorus:CodeFile` |
| discover-tests | Test files by convention | Test inventory per domain | `chorus:TestFile` |
| discover-pages | EJS views, HTML docs | Page inventory per domain | `chorus:Page` |
| discover-endpoints | app.ts route registrations | API endpoints per domain | `chorus:Endpoint` |
| discover-toolchain | package.json, Cargo.toml, binaries | Engine catalog + environments | `borg:Engine`, `borg:Environment` |
| discover-services | LaunchAgent plists, docker-compose | Environments per host | `borg:Environment` |
| discover-storage | Database paths, volumes, named graphs | Resources inside environments | `borg:Resource` |
| discover-network | Listening ports, tunnels, routes | Network map | `borg:NetworkEndpoint` |
| discover-alerts | Prometheus rules, deep-health checks | Monitor+alert coverage | `borg:Monitor`, `borg:AlertRule` |
| discover-commit-pipeline | Hooks, git-queue config | Commit pipeline stages | `borg:PipelineStage` |
| discover-deploy-paths | app-state.sh, bind mounts | Deploy targets + paths | `borg:DeployTarget` |
| discover-security | Ports, auth config, CVEs | Exposure map | `borg:ExposedSurface` |

**The graph knows WHERE to look, not WHAT it found.** Topology lives in the graph (service exists, has port, has health endpoint). Telemetry lives in Prometheus (is it healthy right now?). The herald discovers structure. The graph stores it. Time-series systems measure it.

## Dependency Map

```
Application (Gathering, Chorus)
  uses resources from ↓
Infrastructure (environments on hosts)
  runs engines from ↓
Toolchain (engine catalog: Fuseki, Node, Docker, Prometheus, ...)

  Engine → Environment → Resource
  (Fuseki → fuseki-pods on Library → urn:chorus:ontology graph)
  (MySQL → wordpress-db on Library → wp_posts table)

Commits → Deploys → Infrastructure
  (code versioned → code running → on infrastructure using toolchain)

Alerts & Monitors → Infrastructure
  (watches environments, scores coverage against dependency graph)

Observability → spans all layers (cross-cutting health lens)
Security → spans all layers (cross-cutting risk lens)

Content domain pages show: resources (what I depend on)
Toolchain domain page shows: engines → environments (what we run)
Infrastructure domain page shows: environments per host (what runs where)
```

## Prior Art & Lineage

Borg didn't start here. The concept and its expressions have been developing across multiple documents and sessions.

| Document | Date | Author | Contribution to Borg |
|---|---|---|---|
| Being a Good Borg | March 2026 | Wren | Borg as three expressions: Convergence (separate things are the same thing), Instrumentation (the system measures itself), Self-Awareness (the system generates questions from what it observes). Seven practices. Anti-patterns. "Recording without tending is hoarding. Tending without recording is amnesia." |
| Borg Mitosis | March 2026 | Wren | How the pattern reproduces: Gathering → Chorus → Borg → Akasha → next client. Chorus is acupuncture, not aspirin. Serial growth, not parallel. Each split carries the DNA: ontology-first, value stream modeling, role-based coordination. |
| Borg Assimilation Pattern | 2026-03-18 | Silas | Borg applied to legacy systems: Observe → Assimilate → Adapt → Regenerate. ICD as platform-independent knowledge extraction. Proved on SSIS (13 domains from 13K lines of vendor XML). The Borg separates knowledge from implementation — the ship is irrelevant, the function persists. |
| Codebase Decomposition Spike | 2026-02-21 | Silas | Original borg spike: Tree-sitter parsing, dependency graphs, decomposition engine. Phase 1: prove it on ourselves. Connected Jeff's patent (US9552400B2) to OMG KDM (ISO/IEC 19506). "The most commercially interesting thing in the portfolio." |
| Emergent Evolutionary Architecture | 2026-03-05 | Wren | Paper outline (card #540): architecture that emerges from interaction of constraints and practices, then becomes visible through reflection. Jeff's patent as prior art for structured coordination. Paper drafted but not completed. |

## Emergent Architecture & Reflection

Borg doesn't design architecture. It reveals architecture that already emerged. The system grew organically — LaunchAgents added as needed, Docker containers spun up for specific services, health checks bolted on, hooks accumulated one at a time. 36 hooks later, the collection of solutions is its own architecture. Borg makes that emergent architecture visible and queryable. The graph doesn't prescribe structure — it recognizes it.

This is the connection between the prior art and the current design:

| Prior concept | Current expression |
|---|---|
| Wren's three expressions (convergence, instrumentation, self-awareness) | Engine → Environment → Resource is convergence (same engine, multiple environments). Heralds are instrumentation. Coverage scoring is self-awareness (the system sees its own blind spots). |
| Assimilation pattern (observe → assimilate → adapt → regenerate) | Herald scans (observe) → graph write (assimilate) → human review (adapt) → domain page renders (regenerate the understanding). Same four phases, applied reflexively. |
| Codebase decomposition (tree-sitter → dependency graph → boundary detection) | discover-code, discover-endpoints, discover-pages proved the pattern. The dependency graph IS the borg output. Kade built the heralds. |
| Emergent evolutionary architecture (emerge, then observe) | The system doesn't control emergence. It makes emergence observable. Evolutionary architecture becomes manageable not by preventing drift, but by seeing it. Alert coverage scoring is the metric: are we watching what emerged? |

The key insight from the April 15 session: **borg was already here.** The discover-* endpoints, deep-health.sh, the ontology graph, the domain pages — all borg features that didn't know they were borg features. Naming it didn't create it. Naming it made it visible. That's reflection working on itself.

## What Changed from Previous Design

| Before (2026-04-06) | After (2026-04-15) |
|---|---|
| Single "Observability" service with 6 components | Borg product with 7 domains (5 layers + 2 cross-cutting) |
| Observability nested under Chorus infrastructure | Borg is a peer product alongside Gathering and Chorus |
| Components: metrics, logs, alerts, health, dashboards, instrumentation | Domains: toolchain, infrastructure, commits, deploys, alerts & monitors, observability, security |
| No herald/discovery pattern | Every domain reflectable via discover-* heralds |
| Implementation-specific (Prometheus, Loki, Grafana) | Provider-agnostic model with implementation as herald detail |
| Flat service list | Engine → Environment → Resource hierarchy. Three granularity levels, three domain page views |
| Persistence section shows engine names | Persistence section shows resources: specific graphs, databases, tables with engine and host as properties |
| Alerts as standalone component | Alerts & Monitors with coverage scoring against dependency graph |
| No security domain | Security as cross-cutting capability with exposure mapping |
| No toolchain domain | Toolchain as engine catalog (what we run, separate from where) |
