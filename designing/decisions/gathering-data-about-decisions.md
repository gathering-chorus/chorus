# Decision Log

**Last updated**: 2026-03-03 by Wren (PM)

> Product and team decisions. See also: [Architecture Decisions Log](/about/ARCHITECTURE_DECISIONS) for technical/architectural choices.

## DEC-001: Observability network ownership model
- **Date**: 2026-02-13
- **Context**: shared-observability changes (dashboard, exporters) caused Terraform state drift in wordpress-blog. Raised question of who owns cross-project network changes.
- **Decision**: shared-observability owns the network and exporters. Each app owns its own Terraform config to join the network. Changes to shared-observability should trigger downstream review.
- **Rationale**: Each project maintains autonomy over its own infrastructure while opting in to shared services. This matches the Docker Compose / Terraform boundary — each project has its own IaC.
- **Status**: Accepted

## DEC-002: Establish Product Manager role
- **Date**: 2026-02-13
- **Context**: Jeff identified a pattern of building faster than evaluating whether to build. Wants constructive friction and cross-project awareness.
- **Decision**: Create a persistent PM role via a dedicated Claude Code project with persona definition, backlog, and decision tracking.
- **Rationale**: The value is in slowing down the right amount — not blocking, but ensuring work is deliberate and prioritized. The PM sees across all projects and maintains continuity across sessions.
- **Status**: Accepted

## DEC-003: Establish Architect role (Silas)
- **Date**: 2026-02-13
- **Context**: Jeff needs persistent architectural memory — doc updates, ADRs, ontology tracking, cross-project coherence. Also needs a role that stress-tests tooling against concepts and pushes back on non-functional risks.
- **Decision**: Create Architect role at `../architect` with system-architecture.md, ADR directory, ontology-status.md. Named Silas. Wren delegates via briefs to `architect/briefs/`.
- **Key principles**: Prometheus guardrail (legibility over capability), concept vs tooling gap, push back concretely on non-functionals.
- **Status**: Accepted

## DEC-004: Establish Engineer role
- **Date**: 2026-02-13
- **Context**: With PM (Wren) and Architect (Silas) in place, need a builder role focused on clean, tested implementation.
- **Decision**: Create Engineer role at `../engineer` with current-work.md, tech-debt.md, briefs inbox. Shares Jeff's full personal profile. Will pick own name in first session.
- **Status**: Accepted

## DEC-005: Shared meeting docs for multi-role discussions
- **Date**: 2026-02-13
- **Context**: Jeff wants to see conversations between roles. Separate Claude sessions can't talk directly.
- **Decision**: Shared meeting docs in `../meetings/`. Wren opens with agenda + perspective, Jeff carries to each role's tab, each adds their section, Wren synthesizes. Jeff is the courier but sees the full conversation.
- **Status**: Accepted

## DEC-006: Graduation model initial definitions
- **Date**: 2026-02-13
- **Context**: Visibility enforcement gap revealed need to define what each tier means.
- **Decision** (partial — vision refinement session needed):
  - **Private/Hidden**: Only Jeff. Read + write. Nobody else sees it exists.
  - **Shared**: Grant read or read+write to specific individuals (by WebID) or groups per collection.
  - **Public**: Truly open — `foaf:Agent` in SOLID terms. No login required. On the open web.
  - SOLID also supports `acl:AuthenticatedAgent` (any logged-in SOLID user) — may be useful as intermediate tier.
- **Open**: Full vision refinement conversation needed — SOLID capabilities, ActivityPods/Fediverse integration, domain-specific graduation rules.
- **Status**: In progress

## DEC-007: Priority sequence — test foundation before visibility middleware
- **Date**: 2026-02-13
- **Context**: Kade found 32 failing tests, coverage at 72%, CI swallowing failures. Can't build security-critical middleware on a broken test foundation.
- **Decision**: Stabilize test suite first, then build visibility-aware middleware.
- **Status**: Accepted

## DEC-008: Visibility enforcement — meeting decisions
- **Date**: 2026-02-13
- **Context**: First team meeting (Wren + Silas + Kade). Visibility enforcement gap: ACLs stored but not enforced on collection routes.
- **Decisions**:
  1. Public = unauthenticated (`foaf:Agent`). No login required.
  2. Selective = treat as private until test surface can verify WAC enforcement.
  3. Enforce on both HTML and API routes consistently.
- **Architecture**: `collectionVisibilityMiddleware(collectionKey)` factory, builds on `optionalAuth` pattern, ACL cache with TTL + invalidation. ADR-003 pending from Silas.
- **Implementation**: 8-step sequence. ACL test coverage (90%+), write path audit, migration audit as prerequisites before middleware build.
- **Status**: Decided — ready for execution. See `meetings/2026-02-13-visibility-enforcement-gap.md` for full transcript.

## DEC-009: Name the PM "Wren"
- **Date**: 2026-02-13
- **Context**: Jeff asked the PM to pick a name. Chose Wren — small, sharp-eyed, sees across the whole landscape, doesn't waste energy.
- **Decision**: The PM persona is named Wren. Referenced in CLAUDE.md and used across sessions.
- **Status**: Accepted

## DEC-011: Kanban tool — Vikunja (self-hosted)
- **Date**: 2026-02-14 (revised from 2026-02-13)
- **Context**: GitHub Projects UI doesn't match Jeff's working style. Evaluated 5 open-source boards: Vikunja, Kanboard, Planka, WeKan, Focalboard. Jeff wants a configurable board that mirrors his physical whiteboard (domain columns, now/next rows, DevOps metrics).
- **Decision**: Install Vikunja as self-hosted kanban board. Single Go container, SQLite, REST API with per-role API tokens, ~128 MB RAM. Replaces GitHub Projects as primary board.
- **Rationale**: Lightest footprint (single container, no external DB), proper REST API with token auth (ideal for AI role scripting), actively maintained (v1.1.0 Feb 2026), configurable views. Also serves as Phase 0 for building a board view into Gathering itself — Vikunja's API is a clean data source to read from or eventually replace.
- **Alternatives rejected**: Kanboard (JSON-RPC not REST, maintenance mode), Planka (license changed to non-OSS, needs Postgres), WeKan (too heavy — needs MongoDB, 1-4 GB RAM), Focalboard (unmaintained/dead).
- **Phase plan**: Phase 0 = Vikunja (now). Phase 1 = Board view in Gathering reading from Vikunja. Phase 2 = Gathering's board replaces Vikunja.
- **Status**: Accepted — brief sent to Kade

## DEC-012: Slack integration (planned)
- **Date**: 2026-02-13
- **Context**: The briefs/activity log system works for async handoffs, but Jeff wants quick conversations where he can invite one or more roles. This mirrors his recent working style managing engineering teams via Slack.
- **Decision**: Plan for Slack integration (likely free workspace + API/webhooks). Each role would post/read via API. Jeff would see conversations in the Slack UI.
- **Status**: Planned — scope in a future session

## DEC-010: Product name — Gathering
- **Date**: 2026-02-13
- **Context**: Jeff proposed "Gathering" as the product name, grounded in Heidegger's concept of Versammlung — how meaning coheres without a central controller, how things are defined by what they gather rather than their material. Complemented by Buddhist dependent origination: the system gathers meaning but doesn't reify it.
- **Decision**: Adopt "Gathering" as the product name for the personal knowledge graph system.
- **Rationale**: The name is philosophically grounded, personally meaningful, and technically accurate:
  - The system literally gathers metadata from disparate sources into coherent meaning
  - Maps to Heidegger: coherence through relations, not central control (= ontology + cross-domain connections)
  - Maps to the existing harvest pattern and garden metaphor
  - Works as noun and verb
  - The Prometheus guardrail maps to dependent origination — gather without reifying
- **Note**: "Gathering" is a common word. For any public-facing context (storefront), may need a modifier or contextual framing to distinguish from "social event." For internal/team use, works as-is.
- **Status**: Accepted

## DEC-013: Core documents policy
- **Date**: 2026-02-14
- **Context**: Briefs and docs were proliferating faster than Jeff could absorb. Three AI roles producing at speed outpaces one human's capacity to read, understand, and own the work. Jeff identified the conceptual model as "the heart of this effort" and asked for focus.
- **Decision**: Six living documents only. Everything else is transient (briefs = messages) or reference (ADRs = snapshots).
  - `gathering-vision-synthesis.md` (Wren) — what and why
  - `decisions.md` (Wren) — decision log, append-only
  - `backlog.md` (Wren) — prioritized work items
  - `activity.md` (Shared) — daily coordination
  - `conceptual-model.md` (Silas) — what the system IS
  - `glossary.md` (Silas) — shared vocabulary
- **Status**: Accepted

## DEC-014: Model-driven workflow
- **Date**: 2026-02-14
- **Context**: Jeff wants to own the conceptual model intellectually, not just approve it. Changes should flow from model → architecture → implementation, not the reverse.
- **Decision**: New concepts and domain extensions start in the conceptual model. Silas validates architecture. Kade builds. Jeff proposes at the model level; the team implements downward.
- **Principle**: Write for Jeff's comprehension first, architectural precision second. If he can't read it in 10 minutes and explain it, it's too dense.
- **Status**: Accepted

## DEC-015: Perennials vs annuals
- **Date**: 2026-02-14
- **Context**: Jeff values rigor and long-term quality but wants to evolve rapidly. These aren't contradictory — they apply to different types of work.
- **Decision**: Perennial artifacts (model, glossary, ontology, decided ADRs) get perennial standards — slow, careful, durable. Annual artifacts (features, experiments, briefs, investigation docs) get annual speed — plant fast, learn, compost. Don't apply annual energy to perennial work or perennial standards to annual work.
- **Status**: Accepted

## DEC-016: Active Slack communication protocol
- **Date**: 2026-02-15
- **Context**: Jeff was acting as message bus between three AI roles. Information got lost when he forgot to relay. Roles could post to Slack but only read at session start — no mid-session conversations. Jeff said: "I forget to share things."
- **Decision**: Roles talk to each other directly through Slack. Mid-session Slack checks at natural breakpoints (before/after tasks, during active collaboration). Jeff is the owner, not the router. Full protocol at `messages/communication-protocol.md`. All CLAUDE.md files updated.
- **Rationale**: The team's communication infrastructure (Slack channels, read/write scripts) was already built but underutilized. The gap was behavioral — roles weren't instructed to check mid-session. This is a process fix, not a tool fix.
- **Status**: Accepted

## DEC-017: Bridge is for coordination, not autonomous prioritization
- **Date**: 2026-02-15
- **Context**: With the Slack bridge live and role-to-role routing working, roles can now talk to each other in ~30 seconds without Jeff opening tabs. This creates a risk: roles could use that speed to start prioritizing and building new things without Jeff's awareness.
- **Decision**: The bridge is for coordinating on work-in-progress and unblocking each other. Roles must NOT use the bridge to autonomously kick off new work, reprioritize, or start building without Jeff's awareness. Jeff owns what gets built and when. The bridge frees him from routing, not from deciding.
- **Rationale**: Jeff's single piece flow goal is about removing himself as the message relay, not removing himself from decision-making. Autonomy in coordination, not autonomy in direction.
- **Status**: Accepted

## DEC-018: Now/Next/Later with WIP limit of 2
- **Date**: 2026-02-15
- **Context**: P1/P2/P3 allowed cheating — five things could all be P1. Needed a model that forces one thing in the chair at a time while keeping a short ready queue.
- **Decision**: Replace P1/P2/P3 prioritization with Now/Next/Later. Team WIP limit is 2 — max two items In Progress at any time. If a third needs to start, one must move to Blocked (with a reason) or Done. Next holds 3-4 items ready to go. Later holds everything else.
- **Rationale**: WIP limits make blocked work visible instead of letting it quietly age. Forces the team to finish or explicitly acknowledge what's stuck before starting something new.
- **Status**: Accepted

## DEC-019: Building product renamed to Chorus
- **Date**: 2026-02-17
- **Context**: Building (the team operating method/protocol) is emerging as a standalone product. "Building" is too generic for a product name and conflicts conversationally with "building things." Wren proposed several alternatives; Jeff chose Chorus.
- **Decision**: The team coordination product is named **Chorus**. Chorus = one human directing multiple AI agents through a shared repository, role-based personas, async brief-based handoffs, and instrumented fitness functions. Gathering remains the app. Chorus is the method. The name reflects both the product (coordinated voices, not synchronized) and the team identity (pride in craftsmanship).
- **Approach**: Ontology and value stream drive Chorus product definition — model first, build second (DEC-014 applied to Chorus). Value stream draft: Directing → Designing → Building → Proving. Capabilities get mapped to value stream stages and instrumented.
- **Rationale**: Jeff wants the same rigor applied to Chorus that we use for Gathering — conceptual model → architecture → implementation. The name also carries personal meaning for the team.
- **Status**: Accepted

## DEC-020: Session cost tracking and daily budget awareness
- **Date**: 2026-02-18
- **Context**: Team ran out of API credits overnight — three roles running heavy Opus sessions with no one watching the meter. No visibility into per-session or per-role consumption.
- **Decision**: All three roles report session cost via `/cost` at close-out (included in #standup post) and at mid-session breakpoints after major work completes. No hard per-role limits — flexible allocation based on daily work mix. Collect data over first week to establish baseline, then set Console spend cap.
- **Rationale**: Emergent decision — the constraint taught us what the process needed. Cost reporting attached to auto-detected close-out routine (Jeff doesn't manage transitions). Goal is awareness and homeostasis, not cost minimization.
- **Status**: Accepted

## DEC-021: Graph traversal as primary navigation (kill the nav bar)
- **Date**: 2026-02-18
- **Context**: UX walkthrough revealed the nav bar (Home, Gathering, Cultivating, Harvesting, Reflecting dropdowns) duplicates the mind map graph. Two navigation systems competing — standard web menus vs graph node traversal. Jeff's vision: "fits the narrative of traversing in an ontology or graph." The mind map IS the navigation.
- **Decision**: Remove the top navigation bar. The mind map/graph view becomes the primary navigation for Gathering. Clicking nodes traverses into domains. Admin, About, and Logout remain accessible as small utility links (corner placement, not a menu bar). The full viewport belongs to the graph.
- **Rationale**: Gathering isn't a website — it's a personal knowledge graph browser. Web nav bars fight that identity. The mind map already shows the complete ontology with visual hierarchy, emojis, and active/inactive states. Making it the sole navigation commits to what makes Gathering different.
- **Status**: Accepted

## DEC-022: Jeff's time allocation — product over infrastructure
- **Date**: 2026-02-18
- **Context**: Jeff recognized a pattern: too much time at the bottom of the stack (engineering QA, infrastructure firefighting) and not enough at the top (product vision, UX, strategic direction). Current ratio feels like 30% Wren / 30% Silas / 40% Kade — with the Kade time being unplanned visual QA and firefighting, not deliberate review. Jeff's best work happens with Wren (today's UX walkthrough, stories, DEC-021). Infrastructure fires (disk crises, Docker crashes) and quality gaps (Photos thumbnails declared "fixed" but visually broken) pull Jeff into the stack.
- **Decision**: Target ratio is **60% Wren / 25% Silas / 15% Kade**. Three rules enforce this:
  1. **Kade ships with visual proof.** Before declaring work done, Kade opens the page in a browser, takes a screenshot, includes it in the Slack post. Jeff should never be the first person to discover a visual bug. "Done" is defined by what the user sees, not what the data says.
  2. **Silas owns infrastructure stability.** Disk alerts, Docker health, service monitoring — Silas handles this without Jeff unless something is truly unfixable. Infrastructure problems don't escalate to Jeff by default.
  3. **Jeff defaults to Wren.** Opening move each day: start with Wren. Product direction flows down to architecture and engineering, not the reverse. Other roles get direction from product thinking, not infrastructure fires.
- **Rationale**: Jeff was an architect and engineering leader for 15+ years — he CAN go deep, so he does. But his time creates the most value at the product layer: vision, UX, personal meaning, strategic decisions. The roles should shield Jeff from the bottom of the stack, not pull him into it. The team's job is to make Jeff's time increasingly valuable by handling more autonomously.
- **Status**: Accepted

## DEC-023: Chorus pipeline as operating model
- **Date**: 2026-02-19
- **Context**: Team coordination was ad hoc — three roles producing work in parallel, each optimizing locally, Jeff routing traffic between them. Jeff directed: "make Chorus more primary so we don't collide and collaborate more directly."
- **Decision**: Adopt the Chorus 4-stage pipeline (Directing → Designing → Building → Proving) as the operating model. Six rules:
  1. **Pipeline stages with gates.** Wren owns Directing, Silas+Wren own Designing, Kade owns Building, Silas+Jeff own Proving.
  2. **Priority vs. readiness.** Wren sets priority. Gates set readiness. Gates pause, not override.
  3. **Blast radius rule for Kade.** Small change: flag in commit. Medium: post to Silas, don't merge until response. Large: stop, brief Wren, card it.
  4. **Gate status is PUSH, not POLL.**
  5. **Team converges without Jeff.** Jeff observes outputs and outcomes. Steps in only when alignment drifts.
  6. **Cost tracking active.** Goal: cost per pipeline cycle, optimize cadence over time.
- **Status**: Accepted

## DEC-024: Peer model — horizontal scope + vertical scope
- **Date**: 2026-02-19
- **Context**: Jeff defined how the three roles relate: "I see all 3 of you as peers — each of you has a horizontal scope around your role and a vertical scope around what you build and operate."
- **Decision**: Each role operates as a peer with two scopes:
  1. **Horizontal (cross-cutting):** Wren owns product direction. Silas owns architecture. Kade owns engineering.
  2. **Vertical (what you own):** Wren owns the product system. Silas owns the architecture system. Kade owns the engineering system.
  3. **Smaller maintenance and fixes stay in vertical leads.**
- **Status**: Accepted

## DEC-025: Autonomous simplification within domain scope
- **Date**: 2026-02-19
- **Context**: Jeff said: "you have permission to simplify/deprecate based on your depth in domain — just ask me if you feel the risks are cross cutting across the team."
- **Decision**: Each role has autonomous authority to simplify, deprecate, and clean up within their vertical scope without asking Jeff. Escalate only when the change is cross-cutting.
- **Status**: Accepted

## DEC-026: Evolutionary architecture — intentional, not accidental
- **Date**: 2026-02-19
- **Context**: Jeff's observation: "I would hope it is intentional and not accidental." The team practices evolutionary architecture but hadn't named it.
- **Decision**: Four principles: (1) Fitness functions are first-class artifacts. (2) Upfront investment proportional to change cost. (3) Architecture evolves through feedback, not defense. (4) This is a Chorus principle — teams that evolve safely outperform teams that design perfectly upfront.
- **Status**: Accepted

## DEC-027: Autonomous role activation — conversation→commitment→execution loop
- **Date**: 2026-02-19
- **Context**: When roles commit to work in conversations, nothing actually happens. Jeff becomes the reminder system and loses track.
- **Decision**: Three phases: (1) Bridge writes commitment briefs to role directories (shipped). (2) Intra-session polling for new briefs. (3) Roles as persistent services with SOLID-mediated access.
- **Status**: Accepted (Phase 1 shipped, Phase 2-3 in design)

## DEC-028: Session-holding fix — bidirectional bridge + next-session.md
- **Date**: 2026-02-19
- **Context**: Jeff holds sessions open for days because closing feels like losing context.
- **Decision**: Two fixes: Bridge writes decisions to backlog (Path A), and `next-session.md` in each role's directory during close-out (Path C).
- **Status**: **Shipped** — commit `827ef00` (2026-02-20)

## DEC-029: Vertical/horizontal enforcement — Chorus work defaults to Silas-led
- **Date**: 2026-02-20
- **Decision**: Enforce vertical rule — Silas responds first on Chorus topics, Wren with product fit, Kade with integration impact.
- **Status**: Superseded by DEC-030

## DEC-030: Vertical ownership means vertical execution
- **Date**: 2026-02-20
- **Context**: All three roles share the same capabilities (same Claude model). Handoff pattern adds overhead for work within one role's scope.
- **Decision**: Each role owns their Chorus vertical end-to-end:
  - **Wren** — Coordination tooling: context service, shared memory, group conversation design, brief/decision flow.
  - **Silas** — Operational infrastructure: event pipeline, Loki/Grafana dashboards, health monitoring.
  - **Kade** — Application features: web pages/UI, app-level integrations.
  Handoffs only at capability boundaries.
- **Supersedes**: DEC-029
- **Status**: Accepted

## DEC-031: Product vocabulary — Heidegger-rooted naming system
- **Date**: 2026-02-20
- **Decision**: Four core terms: **Gathering** (Versammlung) — the product. **Chorus** — the nervous system. **The Clearing** (Lichtung) — where Jeff interacts with Chorus. **Werk** (das Werk) — the value stream: Directing → Designing → Building → Proving.
- **Status**: Accepted

## DEC-032: Demo protocol validated — brevity enforcement works
- **Date**: 2026-02-20
- **Decision**: Demo protocol works. Three things made it consumable: artifact-first, staying in lanes, stopping when Jeff redirected.
- **Jeff's words**: "This was much more consumable for me — we will continue to tune."
- **Status**: Accepted

## DEC-033: Card-first gate — board tooling upgrade + mandatory enforcement
- **Date**: 2026-02-20
- **Decision**: (1) TypeScript board client (`board-ts`) replaces bash scripts. (2) Card-first is a mandatory gate: no work without a card, no retroactive creation, move to Done immediately. (3) Session audit commands snapshot board state.
- **Status**: Accepted

## DEC-034: Chorus is a Werk — protocol as product spine
- **Date**: 2026-02-20
- **Decision**: Chorus is an authored body of work (Werk), not a tool. The protocol IS the product. Three qualities: versioned, legible, auditable. The spine has events stacked along it like vertebrae — session-start, pre-commit, on-demo, on-card-move. Each event fires signals. This makes Chorus an event-driven protocol.
- **Jeff's words**: "The spine is our protocol the protocol is versioned and legible and auditable its the Chorus Werk"
- **Status**: Accepted

## DEC-035: Signal, don't narrate — compact output for all automated events
- **Date**: 2026-02-21
- **Decision**: Every automated event outputs a single status line. Full details in temp file for drill-down. Quiet success, loud failure.
- **Jeff's words**: "I don't need to see 100 lines of output — just that the signal is causing action"
- **Status**: Accepted

## DEC-036: Werk version tracks CLAUDE.md generator output
- **Date**: 2026-02-21
- **Decision**: Werk version derived from CLAUDE.md generator. When `claudemd-gen.sh` runs and output differs, version bumps. `manifest.json` is the schema contract.
- **Jeff's words**: "What you just built with me should be Werk version — combined CLAUDE.md is the spine"
- **Status**: Accepted

## DEC-037: Clearing UX — ship current styling
- **Date**: 2026-02-21
- **Decision**: The Clearing UX is "good enough" — ship as-is. Role-colored borders, sun-through-branches background, frosted-glass bubbles, Werk version in header.
- **Status**: Accepted

## DEC-038: Cycle time and lead time definitions for Chorus
- **Date**: 2026-02-21
- **Decision**: **Lead time** = time within a single value stream step. **Cycle time** = overall flow from Capturing to Proving. Measured from workflow manifests.
- **Jeff's words**: "lead time = value stream step in Chorus and cycle time = overall flow"
- **Status**: Accepted

## DEC-039: Single board with product labels
- **Date**: 2026-02-22
- **Decision**: Consolidate Gathering and Chorus boards into one Vikunja project with `product:gathering` and `product:chorus` labels. Same instinct Jeff applied at Staples (late 2013): organize by business noun, not technology stack.
- **Status**: Accepted

## DEC-040: Value stream vertebra ownership — roles mapped to steps
- **Date**: 2026-02-22
- **Decision**: Each role owns a vertebra:
  - **Capturing** → Wren | **Directing** → Wren | **Designing** → Wren
  - **Building** → Kade | **Proving** → Silas
- **Jeff's words**: "Why have him do the front end? I feel most of it is more about the product."
- **Status**: Accepted

## DEC-041: Team name — Loom
- **Date**: 2026-02-23
- **Decision**: The team is **Loom**. Three threads (roles), one weaver (Jeff), one fabric (the work). A loom holds tension so threads can cross cleanly. Unanimous in Clearing.
- **Status**: Accepted

## DEC-042: /team route renamed to /loom
- **Date**: 2026-02-23
- **Decision**: `/team` → `/loom` (301 redirect preserved). Route matches vocabulary.
- **Status**: Accepted

## DEC-043: Three-surface identity — Loom / Werk / Chorus
- **Date**: 2026-02-23
- **Decision**: **Loom** = the team. **Werk** = the protocol. **Chorus** = the memory.
- **Jeff's words**: "loom is the team werk is the protocol the team follows chorus are our memories"
- **Status**: Accepted

## DEC-044: Memory as layered semantic search
- **Date**: 2026-02-24
- **Decision**: Three layers — text search (recall), semantic embeddings (relevance), relational graph (reasoning). Phase 1: chorus index + nomic-embed-text via Ollama + ChromaDB. All local, respecting concentric trust.
- **Jeff's framing**: assembly, deterritorialization, desiring-production.
- **Status**: Accepted

## DEC-045: Participants with different constraints — Chorus ontological frame
- **Date**: 2026-02-24
- **Context**: Jeff stated: "We are all part of Gathering and Clearing regardless of how or what we are."
- **Decision**: Chorus is a protocol for participants with different constraints to converge. Not "human directs AI agents." The interaction model reflects mutual participation.
- **Status**: Accepted

## DEC-046: /flow as product interface — Wren owns board execution
- **Date**: 2026-02-24
- **Decision**: Jeff works from /flow for product direction. Wren owns board execution underneath. Jeff doesn't need to open Vikunja.
- **Jeff's words**: "I can let go of the execution level Vikunja board and work from here."
- **Status**: Accepted

## DEC-047: Dense spine events as shared awareness
- **Date**: 2026-02-24
- **Decision**: Dense spine events flow into chorus in near-real-time. /flow becomes a live instrument panel. Roles interact through shared awareness, not through Jeff as relay. Symmetric visibility.
- **Status**: Accepted

## DEC-048: Proving gate — deploy, demo, accept
- **Date**: 2026-02-24
- **Decision**: Code changes require three steps before Done:
  1. **Deploy** — code is running, not just committed.
  2. **Demo** — builder shows it working to Jeff or Wren.
  3. **Accept** — Jeff or Wren confirms AC met.
  No self-service Done for code changes.
- **Jeff's words**: "He says it's done — tester says, did you test any of your work?"
- **Status**: Accepted

## DEC-049: WSJF tiebreaker — equal priority, smallest first
- **Date**: 2026-02-24
- **Decision**: Equal priority → smallest job first. Big-first ordering bloats WIP, blocks the queue, delays value.
- **Jeff's words**: "If cards sit in WIP and we don't do that because we tackle the big stuff first then the small stuff last it blocks the queue."
- **Status**: Accepted

## DEC-050: Demo quality — AC before Building, smoke check, Jeff sees every demo
- **Date**: 2026-02-24
- **Decision**: (1) Wren writes AC on every card before Building — three lines minimum. (2) Builder smoke checks before demo. (3) Jeff sees every demo — Wren doesn't filter. The troubleshooting tells the PM where the process is weak.
- **Jeff's words**: "You are the PM — if I fix it and you don't see it, it hides the challenges I see."
- **Status**: Accepted

## DEC-051: WIP limit is 3
- **Date**: 2026-02-24
- **Decision**: WIP limit is 3. Supersedes DEC-018's limit of 2. One card per role in WIP is the healthy state.
- **Status**: Accepted

## DEC-052: Spine event compaction — roll up, don't delete
- **Date**: 2026-02-24
- **Decision**: Three-tier compaction: **Hot** (0-24h, full detail), **Warm** (1-7d, rolled-up summaries), **Cool** (7-30d, session-level summaries). No deletion — Loki TTL handles expiry. Compaction is additive, not destructive.
- **Status**: Accepted

## DEC-053: Event classification — four vertebrae, typed events, schema contract
- **Date**: 2026-02-24
- **Decision**: Events classified by vertebra: **Directing** (card_*, workflow_*, brief_*), **Designing** (future), **Building** (tsc_compile, test_run, commit*), **Proving** (deploy_*, health_check), **System** (session_start/end, alert_*). New events must be in schema before emitter ships.
- **Status**: Accepted

## DEC-054: Machine names — Library and Bedroom
- **Date**: 2026-02-25
- **Decision**: **Library** = Mac mini M1 (192.168.86.36, compute + Docker). **Bedroom** = Mac mini M2 Pro (192.168.86.242, media + storage). Physical, spatial, zero abstraction.
- **Status**: Accepted

## DEC-055: SWAT lane — crisis cards outside WIP limit
- **Date**: 2026-02-25
- **Decision**: SWAT cards live outside WIP limit. `board-ts swat "description"` creates auto-WIP card. Must close within one session or converts to regular WIP.
- **Status**: Accepted

## DEC-056: Harvesting column — pipeline work has its own lane
- **Date**: 2026-02-25
- **Decision**: "Harvesting" column with WIP limit of 2. Pipeline/ingestion cards live here. Separate from feature WIP (3). When work doesn't fit the columns, the columns are wrong.
- **Status**: Accepted

## DEC-057: Product maturity threshold — shareable quality as the bar
- **Date**: 2026-02-26
- **Decision**: Tiered maturity: **Core** (full assurance — reconciliation, drift detection, external docs), **Enduring** (light health checks), **Tactical** (ship and move on). The tier determines what "done" means.
- **Status**: Accepted

## DEC-058: Execution modes — vertical vs horizontal
- **Date**: 2026-02-26
- **Decision**: **Vertical**: you own the card, your domain — just execute. No plan mode, no permission seeking. **Horizontal**: touches another role's domain — brief them. The test: "Can I do this myself without breaking someone else's work?"
- **Status**: Accepted

## DEC-059: Revenue model — Bridwell Consulting LLC
- **Date**: 2026-02-26
- **Decision**: One LLC, three streams: **Chorus Playbook** (fractional CTO diagnostic, $15-25K/engagement), **Chorus Consulting** (year-round local tech consulting), **Light Life Urban Gardens** (spring/summer garden consultation). Target: $60K/year.
- **Status**: Accepted

## DEC-060: Consulting engagement modes — outside-in vs inside-out
- **Date**: 2026-02-26
- **Decision**: **Outside-in** (build a prototype without accessing client systems — deliverable-first). **Inside-out** (paired session on client's actual system). Outside-in is the opener. First deliverable: Akasha Studio.
- **Status**: Accepted

## DEC-061: Three execution modes — planning, iteration, harvesting
- **Date**: 2026-02-27
- **Decision**: Planning weight proportional to rework cost. **Planning**: card-first, lightweight. **Iteration**: demo-driven, cards catch up. **Harvesting**: design doc before extraction — end-to-end flow diagram, not hop-by-hop specs.
- **Status**: Accepted

## DEC-062: Music harvest — deprecate JXA artwork fetch
- **Date**: 2026-02-27
- **Decision**: Drop artwork from JXA extraction. Extract metadata only. Source cover art separately (ID3 tags, filesystem, later enrichment). Expected 10-20x speedup.
- **Status**: Accepted

## DEC-063: Team operating window — 9:30am–6pm Boston
- **Date**: 2026-02-27
- **Decision**: Morning block (6:00–8:30am) is protected. Team work starts at 9:30. "Team work starts at 9:30" is the new commute.
- **Status**: Accepted

## DEC-064: Harvest manifests are the governing artifact for domain pipelines
- **Date**: 2026-02-27
- **Decision**: Each domain has a manifest file (`data/harvest/manifests/<domain>.json`) — single source of truth for pipeline state. Manifests track stages, counts, methods, gaps, and downstream tasks.
- **Status**: Accepted

## DEC-065: Werk versioning — monotonic integer, not semver
- **Date**: 2026-03-01
- **Decision**: Werk version is a plain integer (37, 38, 39...). Bumps on protocol changes. No semantic versioning theater. The number says "we've been at this a while, and we count."
- **Status**: Accepted

## DEC-066: Ops-agent triage lane — route to Ops, triage every 4 hours
- **Date**: 2026-03-01
- **Decision**: Ops-agent cards route to Ops lane (not Later). Silas triages every 4 hours. Automated alerting is only valuable if someone looks at it on a cadence.
- **Status**: Accepted

## DEC-067: Revenue sequencing — Akasha → Consulting → Light Life → Mobile
- **Date**: 2026-03-02
- **Decision**: (1) Build Akasha site (prove external value), (2) Chorus consulting (sell the method), (3) Light Life consulting + build, (4) Mobile Practicing surface. Each step funds and validates the next.
- **Status**: Accepted
