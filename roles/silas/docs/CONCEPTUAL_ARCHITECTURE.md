# Conceptual Architecture: One System, Three Layers, Five Modes

**Author**: Silas (Architect) | **Card**: #947 | **WF-099 Step 2**
**Date**: 2026-03-04 | **Companion**: SYSTEM_MODEL.md (Wren, Step 1)

---

## 1. System Shape

One system. Not three products bolted together — one organism with three concentric layers and a circulatory system (Borg) that connects them.

```
                        ┌─────────────────────────────────────────────────┐
                        │                 CHORUS (outer)                  │
                        │  Team memory. Cloud AI. Coordination protocol.  │
     ┌──────────┐       │                                                 │
     │ CAPTURE  │──────▶│   ┌─────────────────────────────────────────┐   │
     │ CHANNELS │       │   │           GATHERING (middle)            │   │
     └──────────┘       │   │   Knowledge graph. Hybrid. 16 domains.  │   │
      SMS, voice,       │   │                                         │   │
      photos, notes,    │   │   ┌─────────────────────────────────┐   │   │
      social, books     │   │   │          SELF (inner)           │   │   │
                        │   │   │   Local AI. Private. Reflection.│   │   │
                        │   │   │   Never leaves the house.       │   │   │
                        │   │   └─────────────────────────────────┘   │   │
                        │   └─────────────────────────────────────────┘   │
                        └─────────────────────────────────────────────────┘
                                              ▲
                                              │
                                         ┌────┴────┐
                                         │  BORG   │
                                         │ nervous │
                                         │ system  │
                                         └─────────┘
```

### Trust Boundaries

Each layer is a trust boundary. Data flows inward freely. Outward flow is filtered — Self's insights move through Jeff, not through a pipe.

| Layer | Trust | Machine | AI | Data Residency |
|-------|-------|---------|-----|----------------|
| Self | Local only | Bedroom (M2 Pro) | Mistral (Ollama) | Never leaves home network |
| Gathering | Hybrid | Library (M1) | Claude (cloud, gated) | Pods on local disk, published selectively |
| Chorus | Cloud | Library (M1) | Claude (cloud) | Git repo, indexed, team-visible |

---

## 2. Layer Architecture

### Self (Inner)

The metacognitive layer. Observes, reflects, surfaces patterns. Does not build.

```
┌─────────────────────────────────────────────────┐
│                  BEDROOM MAC                     │
│                                                  │
│  ┌──────────┐  ┌──────────┐  ┌───────────────┐  │
│  │  Ollama   │  │ Whisper  │  │   LLaVA 7B    │  │
│  │ (Mistral) │  │  (voice) │  │  (posture)    │  │
│  └────┬─────┘  └────┬─────┘  └───────┬───────┘  │
│       │              │                │          │
│       ▼              ▼                ▼          │
│  ┌──────────────────────────────────────────┐    │
│  │           Self Memory Store              │    │
│  │  (future: local-only pod, #939)          │    │
│  └──────────────────────────────────────────┘    │
│       │                                          │
│       │ reads (filtered)                         │
│       ▼                                          │
│  ┌──────────────────────────────────────────┐    │
│  │     Chorus API (read-only, filtered)     │    │
│  │     Sources: memory, stories, decisions  │    │
│  │     http://192.168.86.36:3340            │    │
│  └──────────────────────────────────────────┘    │
└─────────────────────────────────────────────────┘
```

**Interfaces:**
- Ollama API: `http://192.168.86.242:11434` (local inference)
- Whisper: local binary on Bedroom Mac
- Chorus API: `http://192.168.86.36:3340` (cross-machine, read-only)
- Fuseki: `http://192.168.86.36:3030/pods/query` (SPARQL, cross-machine)

**Self reads Chorus through a filter (DEC-068).** Only `memory`, `stories`, and `decisions` source types. No session transcripts, no briefs, no operational data. Self sees what Jeff has thought and decided — not how the team discussed it.

### Gathering (Middle)

The knowledge graph. 16 domains, 16M+ triples, SOLID pods on disk, SPARQL for queries.

```
┌─────────────────────────────────────────────────────────────────┐
│                        LIBRARY MAC                               │
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │  Express App  │───▶│   Fuseki     │    │  Harvest Pipes   │   │
│  │  (port 3000)  │    │  (port 3030) │◀───│  (cron/manual)   │   │
│  └──────┬───────┘    └──────┬───────┘    └──────────────────┘   │
│         │                   │                                    │
│         ▼                   ▼                                    │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                    SOLID Pods                             │   │
│  │  /data/pods/jeff/<domain>/                                │   │
│  │                                                           │   │
│  │  music/ photos/ books/ stories/ notes/ blog/ sexuality/   │   │
│  │  ideas/ projects/ values/ practices/ people/ lists/       │   │
│  │  property/ gallery/ social-posts/ capture/                │   │
│  └──────────────────────────────────────────────────────────┘   │
│         │                                                        │
│         ▼                                                        │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │  LanceDB     │    │  Cloudflare  │    │   Docker         │   │
│  │  (semantic)   │    │  Tunnel      │    │   Compose        │   │
│  └──────────────┘    └──────────────┘    └──────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

**Interfaces:**
- App: `http://192.168.86.36:3000` (Express, EJS views)
- Fuseki SPARQL: `http://192.168.86.36:3030/pods/query`
- LanceDB: embedded in app process (semantic search)
- External: `https://lightlifeurbangardens.com` (Cloudflare tunnel)
- CSS OIDC: `http://localhost:3001` (local auth provider)

### Chorus (Outer)

Team coordination. The method of building is itself a product.

```
┌────────────────────────────────────────────────────────────────┐
│                     CHORUS LAYER                                │
│                                                                 │
│  ┌────────────────┐  ┌────────────────┐  ┌─────────────────┐  │
│  │  gathering-team │  │  Chorus API    │  │  Vikunja Board  │  │
│  │  git repo       │  │  (port 3340)   │  │  (port 3456)    │  │
│  │                 │  │  46K messages   │  │  board-ts CLI   │  │
│  │  CLAUDE.md x3   │  │  indexed        │  │                 │  │
│  │  briefs/        │  │                 │  │                 │  │
│  │  decisions/     │  └────────┬───────┘  └────────┬────────┘  │
│  │  activity.md    │           │                    │           │
│  │  state files    │           ▼                    ▼           │
│  └────────┬───────┘  ┌────────────────────────────────────┐    │
│           │          │         Spine Events                │    │
│           ▼          │  chorus-log.sh → chorus.log → Loki  │    │
│  ┌────────────────┐  └────────────────────────────────────┘    │
│  │  Three Roles    │                                            │
│  │  Wren (PM)      │  ┌────────────────────────────────────┐    │
│  │  Silas (Arch)   │  │      LaunchAgent Services          │    │
│  │  Kade (Eng)     │  │  session-watcher, chorus-api,      │    │
│  └────────────────┘  │  alert-notifier, defect-poller,     │    │
│                       │  ops-agent, fuseki-perf, compact    │    │
│                       └────────────────────────────────────┘    │
└────────────────────────────────────────────────────────────────┘
```

**Interfaces:**
- Chorus API: `http://192.168.86.36:3340/search?q=<term>`
- Board: `http://192.168.86.36:3456` (Vikunja REST API)
- Git: filesystem + `git-queue.sh` (serialized commits)
- Spine: `chorus-log.sh` → `chorus.log` → Promtail → Loki

---

## 3. Data Flow

### The Capture Channel (Ideate)

Many sources, one destination: pods.

```
  SMS ──────────┐
  Voice ────────┤
  Photos ───────┤
  Apple Notes ──┤──▶ Triage ──▶ Pod write ──▶ Fuseki sync
  Social ───────┤       │
  Books ────────┤       └──▶ Seed brief (to role inbox)
  Manual ───────┘
```

**Current state:** Each channel has its own pipeline. SMS comes through a webhook, gets triaged by capture scripts, writes a seed brief. Photos and notes come through harvest scripts. Books come through the upload UI with Claude Vision classification.

**Gap:** No unified intake endpoint. Each pipeline is independent. A future `/capture` API would normalize all channels into one intake → triage → pod write flow.

**Protocol:** All captures write to `/data/pods/jeff/capture/` as raw input. Triage (automated or manual) routes to the appropriate domain pod. Nothing is evaluated at capture — volume and honesty over quality.

### Cross-Layer Data Flow

```
                    INWARD (context flows in)
                    ─────────────────────────
  Chorus ──────────────────▶ Gathering ──────────────▶ Self
  (decisions, briefs,        (pods, SPARQL,            (reflection,
   card context,              domain data,              patterns,
   team memory)               cross-domain links)       meaning)


                    OUTWARD (through Jeff, filtered)
                    ────────────────────────────────
  Self ──▶ Jeff ──▶ Gathering ──▶ Chorus
  (pattern    │     (priority       (decision logged,
   surfaced)  │      changed,        card created,
              │      story written)   brief sent)
              │
              └──▶ Direct action (not mediated by system)
```

**Key principle:** Outward flow goes through Jeff. Self doesn't write to Chorus. Self doesn't create cards. Jeff reads a reflection, changes his mind, and that change flows outward as his action — a reprioritized card, a new story, a conversation with the team.

### Cross-Domain Flow (Think)

```
  Music ◄──────────► Stories
    │                   │
    │    SPARQL joins    │
    ▼                   ▼
  Blog ◄──────────► Ideas ◄──────────► Projects
    │                   │                   │
    ▼                   ▼                   ▼
  Notes ◄──────────► Values ◄──────────► Practices
```

**How it works today:** SPARQL queries across named graphs. Each domain lives in `http://localhost:3000/pods/jeff/<domain>/` graph space. Cross-domain queries use `GRAPH ?g { ... }` patterns. Semantic search (LanceDB) finds resonance across domains by embedding similarity.

**Gap:** The cross-domain connection ratio — how connected is the graph? — is defined as a metric but not instrumented. A SPARQL query could count inter-domain links, but no one runs it regularly or surfaces it.

---

## 4. Borg Architecture

Borg is the convergence engine. It operates across all three layers, detecting patterns and closing loops.

```
┌─────────────────────────────────────────────────────────────┐
│                        BORG                                  │
│                                                              │
│  ┌─────────────────────┐  ┌──────────────────────────────┐  │
│  │   Card Absorption    │  │   Harvest State Tracking     │  │
│  │   (Board Borg)       │  │   (manifest diffing)         │  │
│  │                      │  │                              │  │
│  │   werk-init.sh       │  │   harvest-manifest.json      │  │
│  │   semantic similarity │  │   domain completeness %      │  │
│  │   open ↔ shipped     │  │   gap detection              │  │
│  └──────────┬──────────┘  └──────────────┬───────────────┘  │
│             │                             │                  │
│             ▼                             ▼                  │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              Spine Event Stream                         │  │
│  │   chorus-log.sh → chorus.log → Loki                    │  │
│  │                                                         │  │
│  │   Events: session.*, protocol.*, board.*, harvest.*     │  │
│  │   Roll-ups: daily card flow, WIP trends, deploy freq   │  │
│  └────────────────────────────────────────────────────────┘  │
│             │                                                │
│             ▼                                                │
│  ┌─────────────────────┐  ┌──────────────────────────────┐  │
│  │   Perf Baselines     │  │   Cross-Domain Convergence   │  │
│  │   (perf-baseline.sh) │  │   (NOT YET BUILT)            │  │
│  │                      │  │                              │  │
│  │   Fuseki query times  │  │   Connection ratio           │  │
│  │   Build/deploy SLAs  │  │   Pattern detection           │  │
│  │   Nightly via cron   │  │   Seed generation             │  │
│  └──────────────────────┘  └──────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Where Borg Lives Today

| Component | Where | Trigger | What it does |
|-----------|-------|---------|-------------|
| Board Borg | `werk-init.sh` | Session boot | Matches open cards against shipped work by semantic similarity |
| Spine events | `chorus-log.sh` | Every role action | Logs structured events to Loki-indexed stream |
| Harvest manifests | `harvest-manifest.json` | Post-harvest | Tracks domain completeness, gaps, last-run timestamps |
| Perf baselines | `perf-baseline.sh` | Nightly (LaunchAgent) | Measures Fuseki query times, establishes SLAs |
| Defect poller | LaunchAgent | Continuous | Scans container logs for recurring errors, auto-cards them |

### Where Borg Needs to Go

**Cross-domain convergence detection.** Today Borg only sees cards absorbing cards. It should also see:
- Ideas connecting to stories connecting to blog posts (graph link analysis)
- Domains that were isolated starting to interlink (connection ratio trending)
- Patterns repeating across layers (same structure at different scales)

**Convergence → new seeds.** When Borg detects convergence, it should generate a seed: "These three things you're working on are actually one thing." That seed feeds back into Ideate, completing the cycle.

**Architecture recommendation:** Borg's convergence detection should be a **LaunchAgent service** (like the existing ops-agent pattern), running periodically (every 4 hours or daily), querying across Fuseki (graph connections), Chorus API (team patterns), and spine events (work flow). Output: structured convergence reports written to a pod (`/data/pods/jeff/borg/`) and surfaced on a `/borg` page.

---

## 5. What Exists vs What's Missing

### Exists (solid)

| Component | Layer | Status |
|-----------|-------|--------|
| Express app + EJS views | Gathering | Running, 50+ pages |
| Fuseki + SPARQL | Gathering | 16M+ triples, 33K+ graphs |
| SOLID pods (16 domains) | Gathering | On disk, bind-mounted |
| Harvest pipelines | Gathering | Music, photos, notes, blog, sexuality, stories |
| LanceDB semantic search | Gathering | Embedded, hybrid with FTS |
| Cloudflare tunnel | Gathering | External access |
| CSS OIDC auth | Gathering | Local, 91ms login |
| Chorus API + index | Chorus | 46K messages, searchable |
| Vikunja board + board-ts | Chorus | Kanban with WIP limits |
| Spine events + Loki | Chorus | Structured event stream |
| Three AI roles | Chorus | Wren, Silas, Kade with CLAUDE.md |
| Werk protocol | Chorus | v41, session lifecycle |
| Ollama + Mistral | Self | Running on Bedroom Mac |
| Whisper voice input | Self | Local transcription |
| /reflect page | Self | Conversational interface |
| Posture capture | Self | imagesnap → LLaVA, 5min intervals |
| Board Borg | Borg | Card absorption detection |
| Perf baselines | Borg | Nightly Fuseki benchmarks |
| Observability stack | Borg | Prometheus, Grafana, Loki, 34 alert rules |

### Missing (structural gaps)

| Gap | Layer | Impact | Relates to |
|-----|-------|--------|-----------|
| Unified capture channel | Gathering | Each intake pipeline is independent | #946, #451 |
| Self memory store | Self | Reflect can't persist insights locally | #939 |
| Self → Chorus filtered read | Self | Designed (DEC-068) but not wired | #939 |
| Cross-domain connection ratio | Borg | Defined as metric, not instrumented | SYSTEM_MODEL.md |
| Convergence detection service | Borg | Only card absorption exists today | — |
| Borg → Ideate seed generation | Borg | Cycle doesn't close automatically | — |
| /borg page | Gathering | No visible surface for convergence | — |
| Idea → Project lifecycle UI | Gathering | Designed but not visible in app | #451 |
| /werk instrument layer | Chorus | Pipeline health not self-visible | #621 |

---

## 6. Answers to Wren's Questions

**1. How do data flows between layers actually work?**

HTTP APIs over the home LAN. Gathering → Self: Fuseki SPARQL at `192.168.86.36:3030` (cross-machine). Self → Gathering: Ollama at `192.168.86.242:11434`. Chorus → Gathering: Chorus API at `192.168.86.36:3340` + filesystem (git repo, briefs). No message queues, no event buses — simple request/response over HTTP between two machines on the same network.

**2. Where does Borg's convergence detection live?**

Today: `werk-init.sh` (Board Borg, runs at session boot). Future: a dedicated LaunchAgent service (`com.chorus.borg-convergence`) running every 4 hours, querying Fuseki for cross-domain links, Chorus API for pattern matches, and spine events for work flow analysis. Output to `/data/pods/jeff/borg/convergence-<date>.ttl`.

**3. How does Self's read-only Chorus access work technically?**

Chorus API (`http://192.168.86.36:3340`) already exists and supports search. Self's access is a filtered query: `source=memory,stories,decisions` parameters. The API returns only those source types. Self never sees session transcripts, briefs, or operational data. This is a policy filter on an existing API, not a new service.

**4. What's the architecture of the capture channel?**

Today: N independent pipelines (SMS webhook, harvest scripts, upload UI). Each writes directly to the appropriate pod. Future: one `/api/capture` endpoint that accepts any input (text, image, link, file), runs lightweight triage (classify domain, extract metadata), writes to `/data/pods/jeff/capture/`, and optionally routes a seed brief to the appropriate role. The capture pod is the universal inbox; domain pods are the sorted output.

**5. How does the cross-domain connection ratio get measured?**

A SPARQL query that counts triples where subject and object are in different domain graphs. Something like:

```sparql
SELECT (COUNT(*) as ?connections) WHERE {
  GRAPH ?g1 { ?s ?p ?o }
  GRAPH ?g2 { ?o ?p2 ?o2 }
  FILTER(?g1 != ?g2)
  FILTER(STRSTARTS(STR(?g1), "http://localhost:3000/pods/jeff/"))
  FILTER(STRSTARTS(STR(?g2), "http://localhost:3000/pods/jeff/"))
}
```

Run this daily via a LaunchAgent or as part of the Borg convergence service. Track the ratio over time in a Grafana dashboard. Rising ratio = the graph is becoming a mind, not a filing cabinet.

---

## 7. For Kade (Step 3 Preview)

Three options for making this visible, in order of impact:

1. **Borg convergence view** — a `/borg` page showing: what's converging now (Board Borg output), cross-domain connection count, recent spine event patterns. Low code — mostly SPARQL queries and Chorus API calls rendered in a dashboard layout.

2. **Cycle trace for one real example** — pick an idea that became a card that became a feature that absorbed other ideas. Show the full path through Ideate → Think → Build → Borg with timestamps, artifacts, and connections. Narrative UI, not dashboard.

3. **Cross-domain connection ratio** — instrument and display as a single number with trend. The simplest signal that the system is working as a mind.

Recommendation: option 1 (Borg page) — it's the most novel and gives Jeff a surface he doesn't have yet.

---

*This document is the conceptual architecture. For infrastructure topology, see `infrastructure-constraints.md`. For the value model it implements, see `SYSTEM_MODEL.md`.*
