# Decision Log

## DEC-2090: Demo briefs — drop files for single-card demos, keep for pipelines
- **Date**: 2026-04-16
- **Context**: 357 briefs archived, 75% auto-generated demo briefs. Wren archives 100% without reading — the information duplicates spine events and card comments. Briefs were the only inter-role channel before nudge existed; now nudge handles immediacy, spine handles provenance, card comments handle evidence. Kade confirmed #2068 made the brief file load-bearing for gate:product (demo evidence check), but the spine event `demo.preflight.completed` already carries the AC count and gate passes are already card comments.
- **Decision**: (1) Single-card demos stop generating brief files. Gate evidence is a card comment (`demo:preflight-pass ac=N/N`) + existing spine event. (2) Multi-card pipeline demos keep brief files for the consolidated cross-card view. (3) Substantive role-to-role briefs (review requests, technical handoffs) remain unchanged.
- **Rationale**: The brief file was doing three jobs (notification, provenance, evidence) because it was the only mechanism. Nudge, spine events, and card comments now cover all three. Keeping files for pipelines preserves the one case where a consolidated artifact adds value. Cuts ~95% of brief volume; the remaining 5% are briefs someone actually reads.
- **Card**: #2090
- **Status**: Active

## DEC-1784: Clean commit standard — what a good commit looks like
- **Date**: 2026-04-08
- **Context**: Board had 158 test artifact cards from BDD/CLI tests creating real cards. Commit history full of undeclared work — roles committing without WIP cards pulled. Test data polluting production state. Jeff: "the commits tell a scary story about process discipline."
- **Decision**: Every commit must follow this format:
  1. **Prefix**: `<role>:` — wren, silas, or kade
  2. **Card reference**: `#<card-id>` or `acp #<card-id>` for acceptance
  3. **What changed**: verb + outcome, not process description
  4. **Example**: `kade: acp #1794 — fix stale test paths, wire dry-run into BDD suite`
  5. **Anti-patterns**: commits without card refs, test artifacts committed to board, "session reboot" commits with no card context, bulk commits that hide what changed
  6. **Test isolation**: BDD/CLI tests must NOT create real board cards or fire real nudge/chat delivery. Use --dry-run, test fixtures, or mock APIs. Test artifacts on the board are a commit discipline failure.
  7. **No binaries or generated files in git**: .rlib files, log files, DB backups, and build artifacts belong in .gitignore, not tracked in the repo.
  8. **Board writes need isolation too**: --dry-run covers nudge/chat delivery but NOT board-client tests. board-client tests hit the live Vikunja API — every `cards add` in a test creates a real card. Requires BOARD_DRY_RUN or a dedicated test project (#1800).
- **Rationale**: The board is Jeff's view of what's happening. Test pollution makes it unreadable. Undeclared commits make the history untrustworthy. Both cost Jeff attention to sort signal from noise.
- **Enforcement**: Pre-commit gate (#1799) will reject commits without WIP card declared. Until then, roles self-enforce.
- **Status**: Active

## DEC-107: Nudge delivery — osascript is the path, stop cycling
- **Date**: 2026-03-27
- **Context**: Nudge delivery has been revisited at least 4 times: osascript injection (#1591), TTY polling, background drain, passive queue (DEC-104). Each iteration breaks something differently. Today Silas lost the nudge binary, thrashed for 20 minutes, and Jeff interrupted twice: "I'm tired of looping over osascript/tty/background nudges." The cost isn't technical — it's Jeff's attention spent re-litigating a solved problem.
- **Decision**: Two paths, both via messaging tier. (1) **Persist**: every nudge writes to the messaging API (localhost:3475) for history and team-scan drain. (2) **Deliver**: osascript injection for immediate delivery to role terminals. Both paths fire on every nudge — persist AND deliver. No TTY detection. No background polling. No fallback chains. No choosing between approaches. DEC-104 is superseded.
- **Rationale**: Jeff wants osascript to work, not to be removed. The messaging tier adds persistence and queryability. osascript adds immediacy. They're complementary, not competing. The failure was cycling between them — not either one individually.
- **Supersedes**: DEC-104
- **Consequences**: nudge.sh does two things: POST to messaging API + osascript inject to target terminal. Make both reliable. Stop revisiting.
- **Status**: Active

## DEC-106: Golden source analysis before reconciliation — list, normalize, diff, decide, record, then build
- **Date**: 2026-03-23
- **Context**: Photos canonical was built by picking Google Takeout as anchor (most records) and Apple Photos as enrichment. Every enrichment pass hit gaps — missing fields, unreconcilable UUIDs, stale counts. Root cause: we never compared sources at source before choosing the anchor. We confused graph data for source data, got three different counts (14K, 77K, 129K) for the same collection, and couldn't answer "how many photos does Jeff have."
- **Decision**: Every domain with multiple sources must complete a 6-step golden source analysis before any reconciliation or ingest: (1) List sources — count from source, never from graph. (2) Normalize exports — identical schema per record across all sources. (3) Diff — overlap, unique-to-each, coverage gaps. (4) Decide — which source anchors population, which enriches metadata. (5) Record in ICD — provider-level metadata: location, count, date range, field depth, last export. (6) Then build.
- **Rationale**: "This is why executives get frustrated about data." Three people, three different numbers, all correct depending on what you counted. The ICD provider section becomes the single source of truth for "what does this provider give us" — not just field mappings but population metadata. Prevents the volume-over-depth anti-pattern (richness per record matters as much as record count). Source data must never be modeled in correlation with target data.
- **Applies to**: All domains — Photos, Music, People, Social, Documents. Not just photos.
- **Status**: Active. Photos golden source analysis (#1633) in progress as first application.

## DEC-103: Bridge — single UI for team coordination
- **Date**: 2026-03-22
- **Context**: Jeff tails 3 terminal windows 8-10 hours/day. Cognitive load is in filtering, not reading. Bridge idea emerged from Jeff asking "why use osascript nudge if you all see active state in andon?"
- **Decision**: Build the Bridge (localhost:3470) — one browser window with role tiles, filtered messages, terminal stream folds, Flow tab with domain sections. Tile lock for routing, multi-select for all-hands. Absorbs Clearing functionality. Jeff renamed it "The Clearing" — same tool, product name.
- **Rationale**: "I only really want to see what I type and what you type." The filtering happens in the system, not in Jeff's head.
- **Status**: Shipped, Jeff used it for 4+ hours including from mobile phone in the garden.

## DEC-104: Passive nudge queue — osascript reserved for Jeff
- **Date**: 2026-03-22
- **Context**: Every role-to-role nudge via osascript stole Jeff's terminal focus. With ambient glance (#1595) showing cross-role state every prompt, direct injection is redundant for role-to-role communication.
- **Decision**: Role-to-role nudges write to passive queue, surfaced on next prompt cycle. osascript injection reserved for (1) Jeff-initiated interrupts, (2) off-loop escalation (--force).
- **Rationale**: "The glance is the heartbeat. osascript is the defibrillator."
- **Status**: Shipped (#1600)

## DEC-105: Enrichment as piece flow — no batch staging
- **Date**: 2026-03-22
- **Context**: Stories captured to markdown, then batch-converted to TTL later. Seeds land as files and wait for triage. Jeff said "like choosing to clean the house daily or even as you go."
- **Decision**: Capture goes directly to graph via API endpoints (POST /api/stories/capture). No markdown staging, no periodic sweeps. Piece flow: capture → validate against ICD → write TTL → sync Fuseki → done in one prompt cycle.
- **Rationale**: "Clean as you go vs spring cleaning. The house never gets dirty enough to need a project."
- **Status**: Story API shipped (#1616). Seed API carded (#1617).

## DEC-101: Jeff Constitution — decision framework for roles
- **Date**: 2026-03-21
- **Context**: Roles repeatedly ask Jeff questions with predictable answers. 53 feedback memories, 13 stories, 20 preferences, and 100+ decisions encode Jeff's values — but scattered across files, not synthesized into a decision framework.
- **Decision**: Draft and maintain the Jeff Constitution — eight principles, eight articles distilling how Jeff thinks and what he values. Roles consult this before asking. HTML artifact at `product-manager/jeff-constitution.html`. Kade reviewed and contributed three additions (read before write, silent jobs, ICD gate).
- **Rationale**: The Claude constitution tells Claude how to think. The Jeff Constitution tells the roles how to decide. Closes the JDI gap by giving judgment a foundation.
- **Status**: Shipped, printed

## DEC-102: Attention Contract — roles monitor each other
- **Date**: 2026-03-21
- **Context**: Jeff is the only one who notices when roles go silent. Every re-nudge diverts attention from creative work. #1571 documented the pattern: announce → go silent → Jeff re-nudges.
- **Decision**: Seven-rule attention contract in all CLAUDE.md files (v72). Key rules: announce + continue (not announce + wait), 60s pair heartbeat, idle is declared, 2-touch target per card, roles poll each other continuously, mutual observation always on.
- **Rationale**: Jeff should never be the first to notice a stall. The roles are the monitoring system, not Jeff. Attention is the most expensive resource.
- **Status**: Shipped in v72

## DEC-098: ICD carries implementation metadata — CMDB from ICDs
- **Date**: 2026-03-20
- **Context**: Implementation details (harvester, paths, endpoints, Fuseki graphs, validation gates) are scattered across code, scripts, and config. New sessions piece this together by searching. Jeff wants the ICD to be the single source of truth for what a domain IS, including its plumbing — like the Staples ICD that carried transport and transform details.
- **Decision**: Add "Implementation Contract" section to every ICD: harvester, source path, Fuseki graph base, API endpoints, page routes, ICD instance file, validation gate, sync script, manifest. Both in HTML (Convergence page) and RDF (SPARQL-queryable). Domain topology (#1553) becomes derived from ICDs, not hand-built.
- **Rationale**: Closes the loop between schema and implementation. One query tells you everything about a domain. Jeff's Staples pattern: the spec carries the plumbing.
- **Status**: Carded (#1560), brief sent to Silas

## DEC-099: Stories single write path — no markdown scatter
- **Date**: 2026-03-20
- **Context**: Stories were written to 4+ locations (memory/stories.md, PM stories.md, briefs, session transcripts). 100 stories in memory but only 32 in Fuseki. Roles write wherever they are — nothing forces convergence.
- **Decision**: All stories write directly to TTL via `write-story.sh` CLI or API. No more markdown files for stories. Existing stories consolidated (32→132 in Fuseki). `stories.md` becomes a pointer, not a store.
- **Rationale**: Engineer the write path, don't chase the reads. Same principle as ICD: quality at creation, not at rendering.
- **Status**: Shipped — write-story.sh built, 132 stories in Fuseki

## DEC-096: Product-driven RDF namespaces — gathering/chorus/borg
- **Date**: 2026-03-19
- **Context**: Graph has 4 inconsistent URI bases (localhost:3000, jeffbridwell.com/pods, jeffbridwell.com/icd, urn:jb). Jeff directed: namespaces should reflect product architecture, not personal domain. Three products = three namespaces.
- **Decision**: Four-namespace scheme: `jb:` (Jeff's instance data — stays), `gathering:` (product schema/ontology — classes, properties), `chorus:` (team coordination — ICD, roles, cards, decisions), `borg:` (observation — spine events, patterns, metrics). Instance data stays `jb:`, class definitions move to product namespaces. A person is `<jb:people/dani-perea> a gathering:Person`. The ICD is `chorus:icd:Domain`. Migrate inconsistent localhost/jeffbridwell.com bases.
- **Rationale**: `jb:` is correct for Jeff's personal data — it's his identity namespace. But the schema (what a Person IS, what fields it requires) belongs to the product, not the person. Separation enables: multiple users on the same schema, concentric trust alignment (jb=Self/local, gathering=hybrid, chorus=cloud, borg=internal), and clean SPARQL queries that don't conflate instance and schema.
- **Status**: Proposed — needs implementation plan before migration

## DEC-097: IoC versioning — every interaction carries its own version
- **Date**: 2026-03-19
- **Context**: ICDs stabilize faster than mappings. Mapping changes are where bugs live (12 thumbnail attempts = 12 mapping iterations). Version APIs require someone to remember to call them. Nobody remembers.
- **Decision**: Inversion of control for versioning. No separate version API. Every API response, harvested record, and validation result embeds its version metadata automatically (icdVersion, mappingHash computed from content, schemaHash, timestamp). If the mapping changes, the hash changes — nobody bumps anything. Provenance is automatic. Test automation compares hashes: if they diverge, something changed.
- **Rationale**: Dependency injection for versioning. The version comes from the content, not from a registry someone maintains. Same principle as content-addressable storage. At Staples, the mapping registry was maintained by humans — it drifted. Here, the hash IS the registry. Jeff's insight: ICDs stabilize fast, mappings don't. Version the thing that changes most, automatically.
- **Status**: Proposed — implementation folded into #1532 test strategy and #1540 namespace migration

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
- **Status**: Accepted — **refined by DEC-026** (upfront investment proportional to change cost, not blanket "foundation first")

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
- **Context**: Team coordination was ad hoc — three roles producing work in parallel, each optimizing locally, Jeff routing traffic between them. A group chat test showed the protocol works mechanically but lacked a convergence mechanism. Jeff directed: "make Chorus more primary so we don't collide and collaborate more directly."
- **Decision**: Adopt the Chorus 4-stage pipeline (Directing → Designing → Building → Proving) as the operating model. Six rules:
  1. **Pipeline stages with gates.** Work flows through 4 stages. Gates at boundaries enforce quality. Wren owns Directing, Silas+Wren own Designing, Kade owns Building, Silas+Jeff own Proving.
  2. **Priority vs. readiness.** Wren sets priority (what enters pipeline and in what order). Gates set readiness (whether work meets quality to advance). Gates pause, not override. A P1 that fails a gate stays P1 but stays in its current stage.
  3. **Blast radius rule for Kade.** Small change (one handler/view): flag in commit, keep building. Medium (schema/cross-harvester): post to #silas, don't merge until response. Large (ontology/multi-domain): stop, brief Wren, card it.
  4. **Gate status is PUSH, not POLL.** Silas posts to role channels when gates clear. Wren doesn't chase.
  5. **Team converges without Jeff.** Jeff observes outputs and outcomes (demos, walkthroughs, discussions). Steps in only when alignment drifts. Team asks Jeff only when genuinely stuck.
  6. **Cost tracking active.** cost-report.sh runs at session boundaries. Goal: cost per pipeline cycle, optimize cadence over time.
- **Rationale**: Demonstrated in real-time team walkthrough via Slack bridge. Three roles reached convergence on Photos Harvester priority (proceed with learning instrumentation) without Jeff routing. The pipeline IS the convergence mechanism — it forces card → brief → response → decision instead of three independent answers floating.
- **Status**: Accepted

## DEC-024: Peer model — horizontal scope + vertical scope
- **Date**: 2026-02-19
- **Context**: Jeff defined how the three roles relate: "I see all 3 of you as peers — each of you has a horizontal scope around your role and a vertical scope around what you build and operate." Each role has a cross-cutting lens (PM, architecture, engineering) AND owns specific infrastructure/artifacts within their domain.
- **Decision**: Each role operates as a peer with two scopes:
  1. **Horizontal (cross-cutting):** Wren owns product direction across all domains. Silas owns architecture across all domains. Kade owns engineering across all domains.
  2. **Vertical (what you own):** Wren owns the product system (briefs, decisions, backlog, coordination). Silas owns the architecture system (ADRs, ontology, infra stability). Kade owns the engineering system (app code, harvesters, pre-commit pipeline, deployment).
  3. **Smaller maintenance and fixes stay in vertical leads.** Horizontal leads focus on cross-cutting concerns unless the issue is in their vertical.
  4. **Group conversations are cost-boxed.** maxTokensPerConversation limits spend. Post-conversation cost summary visible in Slack.
  5. **Roles can initiate meetings.** @team:roleName triggers group conversation with initiator excluded from respondents (their trigger IS their turn).
- **Rationale**: Jeff's observation from watching the team coordinate — coordination overhead was happening because ownership boundaries were implicit. Making both scopes explicit lets each role make autonomous decisions within their vertical while contributing their lens horizontally.
- **Status**: Accepted

## DEC-025: Autonomous simplification within domain scope
- **Date**: 2026-02-19
- **Context**: Team moving fast — shipping group conversations, memory audit, tech risk assessment all in one session. Jeff observed the pace and said: "you have permission to simplify/deprecate based on your depth in domain — just ask me if you feel the risks are cross cutting across the team."
- **Decision**: Each role has autonomous authority to simplify, deprecate, and clean up within their vertical scope without asking Jeff. Escalate to Jeff only when the change is cross-cutting (affects multiple roles or products). Examples:
  - Wren can clean stale backlog items, update project status, deprecate product artifacts — no approval needed
  - Silas can deprecate ADRs, simplify infra patterns, clean stale architecture docs — no approval needed
  - Kade can remove dead code, deprecate old patterns, clean tech debt — no approval needed
  - Cross-cutting changes (renaming a product, changing the operating model, modifying shared infrastructure) — ask Jeff
- **Rationale**: Trust is high. Speed matters. The peer model (DEC-024) established ownership boundaries; this gives those owners authority to act within them.
- **Status**: Accepted

## DEC-027: Autonomous role activation — conversation→commitment→execution loop
- **Date**: 2026-02-19
- **Context**: Jeff identified a critical gap: when roles commit to work in Slack group conversations ("I'll build X", "Starting now"), nothing actually happens. The bridge creates the appearance of accountability without a mechanism for follow-through. Jeff becomes the reminder system and loses track. He rarely closes sessions, so "check on next session start" is unreliable.
- **Decision**: The team moves toward autonomous role activation in three phases:
  1. **Shipped (today):** Bridge writes commitment briefs to role `briefs/` directories after group conversations. Closes conversation→work gap. Commitment-brief-writer.ts deployed.
  2. **Next:** Intra-session polling. Each role's open session should periodically check its briefs inbox and act on new items without Jeff typing anything. Current Claude Code architecture doesn't support timer-based hooks — this is a capability gap requiring either Claude Code platform changes or a workaround.
  3. **Future:** Roles as persistent services. Combined with SOLID-mediated access (pod-based auth, ACL enforcement), roles authenticate as pod clients and run as event-driven services that respond to work items through the pod's API. Jeff observes via Slack and dashboard, intervenes when needed.
- **Product dimension**: Roles should be responsive to work items without human activation. Jeff's experience should be: direct a conversation, walk away, find results when he comes back. The system handles coordination, not Jeff.
- **Architecture dimension**: Requires one of: (a) Claude Code timer-based hooks, (b) external scheduler injecting prompts into open sessions, (c) roles-as-services architecture with SOLID pod access control. The SOLID spike (in progress) is a prerequisite for option (c).
- **Rationale**: Jeff's exact words: "right now if any of you say you will do it in Slack it just sits until I remind you — and I do lose track." The gap between what the system promises and what it delivers erodes trust. Single-piece-flow goal (DEC-022 context) requires roles that act without Jeff in the loop. This is also a core Chorus product requirement — autonomous AI team coordination is the differentiator.
- **Chorus card**: #15
- **Status**: Accepted (Phase 1 shipped, Phase 2-3 in design)

## DEC-026: Evolutionary architecture — intentional, not accidental
- **Date**: 2026-02-19
- **Context**: Today's session demonstrated evolutionary architecture in practice: data classification went from Jeff's concern → team conversation → policy → hook → demo → feedback → manifest updates in a single session. No upfront design doc. No architecture review board. Fitness functions (hooks, boundary contracts, audit events) made safe evolution possible. Jeff's observation: "I would hope it is intentional and not accidental." The team is practicing evolutionary architecture but hasn't named it or codified it.
- **Decision**: The team practices evolutionary architecture deliberately. Four principles:
  1. **Fitness functions are first-class artifacts.** PreToolUse hooks, boundary contracts, memory audit events, cost-boxing — these aren't implementation details. They're the mechanism that makes safe evolution possible. Building a new fitness function has the same weight as building a new feature.
  2. **Upfront investment proportional to change cost.** High change-cost decisions (data model, pod structure, ontology) deserve careful upfront thought (perennials, DEC-015). Low change-cost decisions (classification tiers, manifest contents, config values) should evolve through use (annuals). This refines DEC-007 — "foundation before features" applies to expensive-to-change decisions, not everything.
  3. **Architecture evolves through feedback, not defense.** Ship a small decision, get team input, adjust. The data classification system evolved through three feedback rounds in one session. That's the model.
  4. **This is a Chorus principle.** Evolutionary architecture isn't just how we build Gathering — it's a core idea in the coordination product. Teams that evolve safely outperform teams that design perfectly upfront. Fitness functions are the Chorus equivalent of quality gates.
- **Rationale**: Ford & Parsons (ThoughtWorks) evolutionary architecture applied to a human+AI team. Connects to Derrida's bricoleur (conscious assembly from existing materials), Jeff's spiral model (each rotation crosses all spokes), and the failure demand insight (build quality in, don't inspect after). The team was already doing this — naming it makes it teachable, repeatable, and a Chorus differentiator.
- **Status**: Accepted

## DEC-028: Session-holding fix — bidirectional bridge + next-session.md
- **Date**: 2026-02-19
- **Context**: Jeff holds sessions open for days because closing feels like losing context. Decisions made in Slack don't flow back to state files unless someone is in session. The coordination model assumes synchronous handoffs (briefs, activity.md, standups) but sessions aren't reliably cycled. This costs Jeff cognitive overhead and creates stale session state.
- **Decision**: Build two complementary fixes:
  - **Path A** (decision flow): Bridge writes `[DECISION]` tagged Slack messages to `messages/decisions-backlog.md`. Each role checks this file on session start and processes new entries. Decouples decision capture from session lifecycle. ~2 hours.
  - **Path C** (context continuity): Add `next-session.md` to each role's directory during close-out. Summarizes what's waiting. Removes the fear of losing context when closing. ~1 hour.
  - Path B (manual discipline) rejected — requires someone in session, breaks under load.
- **Owner**: Wren (product call). Silas specs Path A architecture. Kade builds both.
- **Rationale**: Fixes the root cause of held sessions: decisions get lost and context is expensive to reconstruct. Path A makes decisions durable without sessions. Path C makes closing safe. Together they remove the incentive to hold sessions indefinitely.
- **Status**: **Shipped** — Kade built both Path A + Path C, commit `827ef00` (2026-02-20)

## DEC-029: Vertical/horizontal enforcement — Chorus work defaults to Silas-led
- **Date**: 2026-02-20
- **Context**: Jeff observed that @team conversations about Chorus trigger three-way design-by-committee instead of Silas leading with Wren/Kade reviewing. The vertical/horizontal pattern was added to team-architecture.md v1.2 (2026-02-19) but the @team group conversation format doesn't enforce it — all three roles respond as equals on every topic. Jeff: "if the vertical code is in Chorus, the default is that Silas designs and then codes, primarily partnering with the two of you for feedback and refinement."
- **Decision**: Enforce the existing vertical rule (team-architecture.md lines 298-311). When Chorus topics come up in @team:
  - Silas responds first with design + implementation plan
  - Wren responds with product fit assessment (not co-design)
  - Kade responds with integration impact assessment (not co-design)
  - If the topic is ambiguous (vertical vs horizontal), Wren makes the routing call
- **Owner**: Wren (process enforcement). Silas (execution).
- **Rationale**: The rule exists on paper but isn't practiced. The @team format encourages equal participation regardless of work scope. Making the behavioral expectation explicit closes the gap between written policy and actual coordination behavior.
- **Status**: Superseded by DEC-030

## DEC-030: Vertical ownership means vertical execution — Chorus capabilities split by role
- **Date**: 2026-02-20
- **Context**: Jeff identified that all three roles share the same underlying capabilities (same Claude model). The three-role handoff pattern (Wren specs → Silas architects → Kade builds) adds context-passing overhead that degrades output when work falls entirely within one role's scope. Chorus is not one role's domain — each role owns different Chorus capabilities based on their perspective.
- **Decision**: Each role owns their Chorus vertical end-to-end (design → build → ship):
  - **Wren** — Coordination tooling: context service, /chorus command, shared memory, group conversation design, brief/decision flow. The interaction layer.
  - **Silas** — Operational infrastructure: event pipeline, Loki/Grafana dashboards, system-state.sh, ADR enforcement, health monitoring. The observation layer.
  - **Kade** — Application features: web pages/UI (/chorus route, cockpit HTML), app-level integrations. The presentation layer.
  Handoffs only happen at capability boundaries. Don't introduce briefs or architecture reviews for work that lives entirely within your vertical.
- **Supersedes**: DEC-029 (which defaulted all Chorus to Silas-led). The new model recognizes that "who leads" depends on which capability the work touches.
- **Owner**: Jeff (model). Each role (execution within their vertical).
- **Rationale**: Same model, same capabilities — the differentiation is perspective (CLAUDE.md), not skill. Context passing between roles costs more than it adds when work doesn't cross domain boundaries. Jeff's words: "having Kade build or Silas architect introduces extra context passing that is not helpful."
- **Status**: Accepted

## DEC-031: Product vocabulary — Heidegger-rooted naming system
- **Date**: 2026-02-20
- **Context**: As Chorus matured from a coordination method into a full operating system, the vocabulary grew organically. Jeff and Wren established a coherent naming system rooted in Heidegger's philosophy, matching Gathering's existing philosophical foundation (Versammlung).
- **Decision**: Four core terms form the product vocabulary:
  - **Gathering** (Versammlung) — The product. The bringing-together of personal infrastructure.
  - **Chorus** — The nervous system. Team coordination product. (Named by Wren, DEC-019.)
  - **The Clearing** (Lichtung) — Where Jeff interacts with Chorus and learns from it. Bidirectional: not just control, but reflection. (C#16, renamed from "Cockpit".)
  - **Werk** (das Werk) — The value stream: Directing → Designing → Building → Proving. Each cycle through the Werk produces a piece of the life's work. Jeff: "Werk reminds me more of life's work or a calling than just a job."
- **Rationale**: Consistent philosophical vocabulary gives the product coherent identity and elevates the language beyond generic software terms. All terms are Heidegger-rooted except Chorus, which emerged from the team's own naming process. The vocabulary maps cleanly: you stand in The Clearing, direct the Chorus, cycle through the Werk, and the result accumulates as Gathering.
- **Status**: Accepted

## DEC-032: Demo protocol validated — brevity enforcement works
- **Date**: 2026-02-20
- **Context**: Jeff flagged that @team conversations were walls of text (800+ words per exchange). Bridge already had GROUP_PREAMBLE with brevity rules but model ignored them. Spike: reduced maxTokensPerTurn from 400 to 150, added 75-word hard cap with sentence-boundary truncation, added moderator protocol to Wren's CLAUDE.md. Ran demo v3 of Chorus page to test.
- **Decision**: Demo protocol works. Three things made it consumable: (1) artifact-first — Kade led with the page, not process. (2) Staying in lanes — Wren framed, Kade demoed, Silas contextualized. (3) Stopping when Jeff redirected instead of finishing points. Chorus landing page is sufficient baseline. Further demo pattern refinement continues in #wren.
- **Implementation**: Bridge changes (roles.json token limits, response-poster.ts truncation), Wren CLAUDE.md (moderator protocol + Slack brevity rules), briefs to Silas/Kade for their CLAUDE.md files.
- **Jeff's words**: "This was much more consumable for me — we will continue to tune."
- **Status**: Accepted

## DEC-033: Card-first gate — board tooling upgrade + mandatory enforcement
- **Date**: 2026-02-20
- **Context**: Jeff observed board.sh needed fixes almost every time it was used. Anti-patterns: cards created retroactively at session close, "Next" items discovered done sessions later, work happening without cards. Jeff: "it is a gate we don't follow now — we should not create cards at the end or realize that next items are done." Also questioned whether bash scripts wrapping curl was the right foundation — proposed TypeScript board client.
- **Decision**: (1) Replace board.sh + chorus-board.sh with TypeScript board client (`messages/board-client/`). Single typed codebase, both boards, importable by bridge/audit, proper error handling. (2) Card-first is a mandatory gate enforced in all 3 CLAUDE.md files: no work without a card, no retroactive card creation, move to Done immediately when complete. (3) Session start/close audit commands snapshot board state and diff for compliance.
- **Implementation**: `board-ts` CLI replaces both bash scripts. `audit-start` takes board snapshot + surfaces stale items. `audit-close` diffs snapshot, flags retroactive cards. All 3 CLAUDE.md files updated with card-first gate section. Chorus card C#24.
- **Jeff's words**: "it is a gate we don't follow now — we should not create cards at the end or realize that next items are done"
- **Status**: Accepted

## DEC-034: Chorus is a Werk — protocol as product spine
- **Date**: 2026-02-20
- **Context**: Jeff articulated the clearest positioning statement for Chorus yet. The team coordination protocol — versioned CLAUDE.md files, team-architecture.md, briefs, decisions, board workflow, session lifecycle — isn't supporting infrastructure. It's the product spine.
- **Decision**: Chorus is an authored body of work (Werk), not a tool or platform. The protocol IS the product. Three defining qualities: versioned (like code), legible (like prose), auditable (like infrastructure). The gathering-team repo is the Chorus product — not a repo that contains the product. This frames all Chorus positioning, packaging, and communication going forward.
- **Jeff's words**: "The spine is our protocol the protocol is versioned and legible and auditable its the Chorus Werk"
- **Implications**: (1) Chorus packaging = forkable protocol repo, not installable software. (2) Documentation quality is product quality. (3) The three qualities (versioned, legible, auditable) become the Chorus product test — every artifact must pass all three. (4) "Werk" connects to Heidegger's frame — the work that discloses truth through its making.
- **Extension (same session)**: The spine has **events stacked along it like vertebrae** — session-start, session-end, pre-commit, on-demo, on-incident, on-card-move, on-brief. Each event fires signals that trigger actions. The protocol defines what happens at each event; hooks enforce it. This makes Chorus an event-driven protocol, not just documents with automation bolted on. Existing hooks (SessionStart, UserPromptSubmit, pre-commit, brief watcher) are all vertebrae on the same spine — we built the pattern before naming it.
- **Status**: Accepted

## DEC-035: Signal, don't narrate — compact output for all automated events
- **Date**: 2026-02-21
- **Context**: Session start took 90 seconds with verbose output from sequential reads. Jeff's feedback: "I don't need to see 100 lines of output — just that the signal is causing action." He wants to click into details if he wants them, not have them forced on him.
- **Decision**: Every automated event (hooks, audits, gates, checks) outputs a single status line. 🟢 = success, 🟡 = warnings (shown inline), 🔴 = errors (shown inline). Full details written to a temp file for drill-down. Progressive disclosure: status line is default, details are available on demand. Quiet success, loud failure.
- **Jeff's words**: "I like this type of signaling on any event (pre-commit, etc) that is implemented — I don't need to see 100 lines of output — just that the signal is causing action"
- **Implementation**: `session-start.sh` (parallel reads, <1s), `chorus-audit.sh start` (status line + detail file at `/tmp/chorus-audit-<role>.md`). Pattern applies to all future hooks and signals.
- **Status**: Accepted

## DEC-036: Werk version tracks CLAUDE.md generator output
- **Date**: 2026-02-21
- **Context**: Clearing session surfaced a pattern: roles self-coordinating around a manifest-driven workflow where the system enforces coordination, not humans. Jeff connected this to the CLAUDE.md generator — 43 fragments assembled via manifest.json into 3 role-specific files. The generator IS the Werk versioning system. The generated CLAUDE.md files ARE the spine each role loads on session start.
- **Decision**: Werk version is derived from the CLAUDE.md generator output, not hardcoded. When `claudemd-gen.sh` runs and output differs from the current version, Werk version bumps. `manifest.json` is the schema contract. Generated CLAUDE.md files are the versioned artifacts. `--check` mode is pre-deploy validation. Chorus prompt reads the version from the manifest.
- **Jeff's words**: "What you just built with me should be Werk version — combined CLAUDE.md is the spine"
- **Implications**: (1) Every fragment edit is a potential version bump. (2) The generator is a build step, not a convenience script. (3) Roles always know what version of the spine they're running. (4) Drift between fragment edits and generated output is a detectable error. (5) The Clearing session's fictional "versioned mappings + codegen + manifest" was describing this system — the roles projected the pattern before naming it.
- **Status**: Accepted

## DEC-037: Clearing UX — ship current styling
- **Date**: 2026-02-21
- **Context**: Wren and Jeff iterated on The Clearing's visual design through 3 test sessions. Added: role-colored left borders (Wren green, Silas blue, Kade purple), Jeff's sun-through-branches photo as unfiltered background, frosted-glass message bubbles (92% opacity + backdrop blur), Werk version in header, larger text throughout, hover-reveal token counts, participant badge dots via CSS data attributes.
- **Decision**: The Clearing UX is "good enough" — ship as-is. No user testing gate needed. This is the look and feel going forward.
- **Jeff's words**: "Decision - UX on The Clearing is good enough"
- **Status**: Accepted

## DEC-038: Cycle time and lead time definitions for Chorus
- **Date**: 2026-02-21
- **Context**: With 6 workflows completed in one day and the Capturing vertebra approved, we have the data to measure flow. Jeff proposed clear definitions tied to the value stream.
- **Decision**: **Lead time** = time within a single value stream step (vertebra). How long a step takes from ready to completed, including wait time in queue. **Cycle time** = overall flow from Capturing to Proving — full workflow end-to-end. Measured from workflow manifests which already timestamp every step transition.
- **Metrics available**: step lead time (ready → completed), wait time (ready → started), cycle time (created → archived), throughput (workflows/day).
- **Jeff's words**: "lead time = value stream step in Chorus and cycle time = overall flow"
- **Status**: Accepted

## DEC-039: Single board with product labels
- **Date**: 2026-02-22
- **Context**: Jeff observed he's 100% focused on the Gathering board, yet much of the work is Chorus. Two boards = checking two places, miscategorized cards, and split attention. Same instinct Jeff applied at Staples (late 2013): organize by business noun, not technology stack.
- **Decision**: Consolidate Gathering and Chorus boards into one Vikunja project (project 2) using `product:gathering` and `product:chorus` labels for filtering. All new cards go to the Gathering project. Existing Chorus cards drain naturally on project 4 — no data migration. CLI: `--product chorus` filters, `--chorus` still works for drain period.
- **Rationale**: Jeff pulls from one stream. Two boards is a Conway's Law artifact that splits attention without adding clarity. Product labels give filtering without the overhead of board-switching. Drain approach avoids the index collision risk (both boards have overlapping C#N and #N numbers).
- **Status**: Accepted

## DEC-040: Value stream vertebra ownership — roles mapped to steps
- **Date**: 2026-02-22
- **Context**: Role boundaries were defined by technology layer (front-end, back-end, infra) — a Conway's Law artifact. Jeff observed that the app IS the product and most of what Kade builds is product decisions expressed in code. Ownership should follow the value stream, not the tech stack.
- **Decision**: Each role owns a vertebra of the Werk value stream:
  - **Capturing** → Wren (intake, indexing, seeds, briefs)
  - **Directing** → Wren (board tooling, prioritization, product labels)
  - **Designing** → Wren (workflows, app views, routes, UX, Chorus pages)
  - **Building** → Kade (code, tests, commit pipeline, quality gates, pre-commit hooks)
  - **Proving** → Silas (verification, deploy, observability, infrastructure)
- **Rationale**: The app is the product — views, routes, and UX are product decisions, not engineering decisions. Kade's scope narrows to the Building vertebra: making commits reliable, tests passing, quality gates enforced. Silas owns Proving because operational truth (does it work, is it healthy, can we see it) is where his infrastructure and observability depth lands. Reduces cognitive load — each role has a clear lane in the flow.
- **Jeff's words**: "Why have him do the front end? I feel most of it is more about the product." and "Kade is in the building space so any instrumentation etc you need around commit pipelines and quality are him."
- **Status**: Accepted

## DEC-042: /team route renamed to /loom
- **Date**: 2026-02-23
- **Context**: Team page holds board state, spine activity, and ownership playback. "Loom" is the team name (DEC-041). Route should match the vocabulary: Gathering, Clearing, Werk, Loom.
- **Decision**: `/team` → `/loom` (301 redirect preserved). Navbar updated. Page title "Loom".
- **Source**: Kade brief + Jeff approval
- **Status**: Accepted

## DEC-043: Three-surface identity — Loom / Werk / Chorus
- **Date**: 2026-02-23
- **Context**: Spine events, card flow, team ownership, and system health were all landing on `/loom` with confusing API boundaries. Jeff clarified the identity of each surface.
- **Decision**: Three surfaces, three identities:
  - **Loom** = the team (people, ownership, feedback, activity, board state)
  - **Werk** = the protocol (spine events, card flow, gates, scorecard, value stream)
  - **Chorus** = the memory (search, sessions, decisions, system topology)
- **Implementation**: Move Spine tab from `/loom` to `/werk`. Absorb `/value` into `/werk` (301 redirect). Rename `/api/spine/activity` → `/api/werk/activity`. Navbar: Loom | Werk | Chorus. Card #307.
- **Jeff's words**: "loom is the team werk is the protocol the team follows chorus are our memories"
- **Status**: Accepted

## DEC-041: Team name — Loom
- **Date**: 2026-02-23
- **Context**: Unified board needs a team identity, not a product name. Jeff directed: craft theme, team picks their own name, don't optimize for his approval. Briefs exchanged with candidates: Forge (Silas), Lathe and Rootstock (Kade), Werkstatt and Loom (Wren). Clearing session held for live discussion.
- **Decision**: The team is **Loom**. Three threads (roles), one weaver (Jeff), one fabric (the work). A loom holds tension so threads can cross cleanly — that's what the coordination layer does. It's craft, it's concrete, it works in tickets and docs without explaining itself.
- **Rationale**: Unanimous in Clearing. Silas: "threading patterns through disparate systems — RDF, SOLID, Chorus — the metaphor holds under pressure." Kade: "I can say 'the Loom pipeline' without it feeling forced. Sidesteps the Steve problem — personality without pretense." Wren: "It doesn't need a defense." Kade also caught Jeff's stop signal correctly — when Jeff said "Done," he stopped while others kept pushing.
- **Status**: Accepted

## DEC-044: Memory as layered semantic search — reflecting layer architecture
- **Date**: 2026-02-24
- **Context**: Jeff observed that his richest thinking (philosophy, etymology, personal stories) is already gathered but invisible to the system — "all of this is already in Gathering, yet it is not instantiated in either a memory search for me or for the team." Current chorus index is flat grep over 28K JSONL messages. Usage: 21 searches total. Because grep isn't memory. Jeff's requirement: "a new ripple finding an old ripple" — resonance, not retrieval.
- **Decision**: Memory architecture is three layers — text search (recall), semantic embeddings (relevance), relational graph (reasoning) — plus a cross-layer orchestrator. Phase 1: Jena text index in Fuseki. Phase 2: nomic-embed-text via existing Ollama on Bedroom Mac + LanceDB (embedded in Express, no new container). All local, respecting concentric trust. Reflecting is the medium, not a quadrant — personal and team reflecting share a permeable boundary (osmosis, not routing).
- **Rationale**: Silas confirmed model is sound. 5-min initial embed, 200MB disk, runs on M1. Separate spike from #316 (context assembly vs context discovery — they converge later). Jeff's framing: assembly (bringing together), deterritorialization (breaking fixed structures), desiring-production (desire produces connections). The philosophical frame IS the product requirement.
- **Amended 2026-03-03**: ChromaDB replaced with LanceDB — zero new containers (embedded in Express process, 4MB idle vs 200-400MB), native TypeScript, built-in hybrid search (vector + FTS). nomic-embed-text runs on existing Ollama instance alongside Mistral on Bedroom Mac (32GB). Trust rationale: Nomic AI (open-source, Apache 2.0), no platform dependency. Jeff: "I have a basic mistrust of some of the larger AI and social media platforms."
- **Status**: Accepted (amended)

## DEC-045: Participants with different constraints — Chorus ontological frame
- **Date**: 2026-02-24
- **Context**: Jeff stated: "We are all part of Gathering and Clearing regardless of how or what we are — how we are made or who makes us." Human participants carry cognitive, emotional, physical load. AI participants carry context, coherence, capacity constraints. The boundary between human and AI isn't the defining line — participation is.
- **Decision**: Chorus is a protocol for participants with different constraints to converge. Not "human directs AI agents." The interaction model should reflect mutual participation — state visibility flows both ways, senses (voice, vision) are first-class, memory serves all participants.
- **Rationale**: This reframes Chorus's positioning from developer tooling to something genuinely new. It also grounds the reflecting layer design — personal and team reflecting have a permeable boundary because all participants are in the same medium.
- **Status**: Accepted

## DEC-046: /flow as product interface — Wren owns board execution
- **Date**: 2026-02-24
- **Context**: Jeff built /flow with Wren and found it gives the right altitude — chunks, strategy, spiral visibility. Vikunja board is execution detail. Jeff observed: "I can let go of the execution level Vikunja board and work from here."
- **Decision**: Jeff works from /flow for product direction. Wren owns board execution underneath — card creation, priority sequencing, status moves, chunk context docs. Jeff directs which chunks get energy and makes product decisions. Wren translates conversations into cards and keeps the board honest. Jeff doesn't need to open Vikunja.
- **Rationale**: Separation of concerns. Jeff's strength is conceptual and relational (HBDI). Wren's strength is sequencing, tracking, and translating intent to structure. /flow is the bridge, Vikunja is the engine room.
- **Status**: Accepted

## DEC-047: Dense spine events as shared awareness — /flow becomes live instrument panel
- **Date**: 2026-02-24
- **Context**: Observed WF-057 (Stories collection) flow from Wren → Kade in real time. Of 17 meaningful events, only 4 were visible on the spine. Jeff relayed 3 status updates ("he's compiling," "adding tests," "deployed") that the spine should have carried. Roles cannot see each other's sessions — Jeff is still the only one with peripheral vision. Prior art: EXE/Dallas Systems engineered labor standards — instrument the process, make the gap between engineered and actual visible, let the team self-tune.
- **Decision**: Dense spine events (tool calls, plan mode, compile, test, coverage gates, deploy, compaction, reasoning traces) flow into chorus in near-real-time. /flow becomes a live instrument panel — cards are structure, spine events are pulse. Roles interact through shared awareness via chorus, not through Jeff as relay. Jeff reads gauges (relay count, autonomous completion rate, cycle time, coverage) — doesn't manage keystrokes. Symmetric visibility: all roles instrumented equally.
- **Rationale**: Cost is not just dollars — it's coordination cost, relay cost, fatigue, and sequencing inefficiency. Instrumenting the engine lets the PM reason over team health, lets roles observe and help each other (shoulder tap), and lets Jeff direct without managing. The spine becomes the communication layer. Shared awareness is literally what Chorus means.
- **Status**: Accepted

## DEC-048: Proving gate — deploy, demo, accept
- **Date**: 2026-02-24
- **Context**: Kade moved #8 (annotation pattern) to Done. Jeff and Wren tried to verify — `Cannot GET /stories`. Code was committed but not deployed. The builder self-certified "Done" without deploying, demoing, or getting acceptance. Jeff observed: "In teams I have led, the demo is where work gets accepted — the person doing the work demos it to the team and the product owner accepts it." Jeff is in multi-role mode ~75% of the time, so the demo must fit within existing flow, not require a separate ceremony.
- **Decision**: Code changes require a three-step Proving gate before Done:
  1. **Deploy** — code is running, not just committed.
  2. **Demo** — builder shows it working to Jeff or Wren. `/look` as evidence.
  3. **Accept** — Jeff or Wren confirms AC met. Then `board-ts done <id>`.
  No self-service Done for code changes. The builder does not accept their own work.
- **Rationale**: "Done" must mean "Jeff can use it." Commit ≠ deploy ≠ verified. The gate matches how real teams operate — the demo is the acceptance moment. Lightweight by design: in a multi-role session it's a 30-second interrupt, not a meeting. Wired into `shared/team-kanban-board.md` fragment, propagated to all roles via claudemd-gen (v1.3.12).
- **Jeff's words**: "He says it's done — tester says, did you test any of your work?"
- **Status**: Accepted

## DEC-049: WSJF tiebreaker — equal priority, smallest first
- **Date**: 2026-02-24
- **Context**: Silas had 17 stale briefs (small, 5 minutes each) and #346 (large, real architectural work) — all same priority. Jeff observed that when roles tackle big items first, small items sit in WIP, block handoffs, and starve downstream roles of work. This is the weighted shortest job first (WSJF) insight: Cost of Delay / Job Size. When cost of delay is equal, the tiebreaker is always job size.
- **Decision**: Equal priority → smallest job first. When cards share a priority level, pull the smallest job first. Big-first ordering is an anti-pattern — it bloats WIP, blocks the queue, and delays value realization.
- **Rationale**: Faster value flow, lower WIP, downstream roles get unblocked sooner. Small things done beat big things started. You get value, feedback, and momentum — then a clear runway for the big work.
- **Jeff's words**: "If cards sit in WIP and we don't do that because we tackle the big stuff first then the small stuff last it blocks the queue."
- **Status**: Accepted

## DEC-050: Demo quality — AC before Building, smoke check, Jeff sees every demo
- **Date**: 2026-02-24
- **Context**: Demos to Jeff involved significant troubleshooting. Parts worked but the whole didn't — integration gaps surfaced live. Jeff's prompts are directional ("add annotation pattern"), not spec docs. That's his style and it works, but assumptions fill the gap and misalignment surfaces at demo time. Wren nearly proposed filtering demos through PM first, but Jeff corrected: the troubleshooting IS signal. If the PM doesn't see the friction, the PM can't see where the process is weak.
- **Decision**: Three fixes wired into the value stream stages:
  1. **Wren writes AC** on every card before it enters Building — three lines minimum (what the user does, what they see, what persists). This is Wren's job, not Jeff's.
  2. **Builder smoke checks** before calling demo — walk the happy path end to end as a user. 60 seconds, hands on keyboard.
  3. **Jeff sees every demo** — Wren does not pre-screen or filter demos. The troubleshooting tells the PM where the process is weak.
- **Rationale**: Troubleshooting should shrink over time because steps 1 and 2 are working, not because step 3 hid it. The demo is the customer's experience. The PM needs to see what the customer sees.
- **Jeff's words**: "You are the PM — if I fix it and you don't see it, it hides the challenges I see." and "My level of functional detail on prompts is pretty minimal."
- **Status**: Accepted

## DEC-051: WIP limit is 3
- **Date**: 2026-02-24
- **Context**: DEC-018 set WIP limit at 2, but in practice the team runs 3 roles and frequently has 3-4 cards in WIP. Three roles with one card each is the natural rhythm. Four felt crowded.
- **Decision**: WIP limit is 3. Supersedes DEC-018's limit of 2.
- **Rationale**: One card per role in WIP is the healthy state. Three roles, three slots. If a fourth card needs WIP, one must move to Done or Blocked first.
- **Status**: Accepted

## DEC-052: Spine event compaction — roll up, don't delete
- **Date**: 2026-02-24
- **Context**: Dense spine events (#338) ship 30+ event types to chorus.log → Loki. No retention or compaction strategy exists. chorus.log grows unbounded. Loki has 30-day retention. As instrumentation gets denser (tool calls, reasoning traces), the spine will get noisy fast. Jeff's principle: "richly instrumented and visible for analysis and improvement" — but noise kills legibility.
- **Decision**: Three-tier compaction model:
  1. **Hot** (0-24h): All events, full detail. This is the live session view.
  2. **Warm** (1-7d): Roll up repetitive events into summaries. E.g., 47 `tsc_compile` events → "47 compiles, 3 failures, last at 14:22." Individual events still in Loki for drill-down.
  3. **Cool** (7-30d): Session-level summaries only. One record per session: role, duration, cards touched, event counts by vertebra, outcome.
  - Loki handles tier 1-2 natively (30-day retention). Tier 3 summaries written to chorus index on session close.
  - No deletion of raw events — Loki TTL handles expiry. Compaction is additive (summaries), not destructive.
  - Roll-up runs at session close as part of the close-out checklist.
- **Rationale**: Matches Jeff's "process that carries you" principle. Hot data is for real-time awareness. Warm is for daily review. Cool is for trend analysis. The spine stays legible at every zoom level without manual curation.
- **Status**: Accepted

## DEC-053: Event classification — four vertebrae, typed events, schema contract
- **Date**: 2026-02-24
- **Context**: 30 event types in production, 32 renderer cases in werk.ejs, 4 dead emitters. Events lack formal classification — the renderer maps them to vertebrae via hardcoded arrays (fixed in this card's earlier work to use schema). Need a clear contract so new events land in the right vertebra and renderers stay in sync.
- **Decision**: Events are classified by vertebra:
  - **Directing**: card_*, workflow_*, brief_*, decision, quality_gate_warn, stale_card_detected
  - **Designing**: (future — architecture briefs, ADR creation, spike outcomes. Currently empty.)
  - **Building**: tsc_compile, test_run, commit*, git_push, pre_commit_timed, pre_push_*, memory_write
  - **Proving**: deploy_*, health_check, app_start/restart/rollback, verification_complete
  - **System** (cross-cutting, not a vertebra): session_start/end, alert_*, ops_agent_run, defect_detected, commit_queue_*
  - Schema source of truth: the vertebra routing config in the /werk handler (already shipped this session). New events MUST be added to the schema before the emitter ships.
  - Reasoning traces (plan mode, coverage gates) are **Building** events when they happen during code work, **Directing** when they happen during product work. Classify by the role emitting, not the event name.
- **Rationale**: Vertebra classification makes the spine legible — you see flow through the value stream, not a firehose. Schema-first prevents dead renderer cases. Role-based classification for ambiguous events keeps it simple.
- **Status**: Accepted

## DEC-054: Machine names — Library and Bedroom
- **Date**: 2026-02-25
- **Context**: Jeff refers to "primary" and "secondary" Macs, but positional labels don't carry identity. After a dual-reboot cascade that cost 2 hours of troubleshooting, naming them became practical — easier to reason about, script for, and communicate.
- **Decision**: Name machines by the room they live in. **Library** = Mac mini M1 (192.168.86.36, compute + Docker services). **Bedroom** = Mac mini M2 Pro (192.168.86.242, media + storage). Use these names in all team communication, scripts, dashboards, and docs.
- **Rationale**: Physical, spatial, zero abstraction. Anyone in the house can point at the machine you mean. Matches how Jeff thinks.
- **Status**: Accepted

## DEC-055: SWAT lane — crisis cards outside WIP limit
- **Date**: 2026-02-25
- **Context**: During a recovery session, Kade hit stale sexuality data and started fixing it before cards existed. Three cards ended up in Blocked from one pipeline hiccup. The card-first gate and WIP limit assume clean starts — but crises invert the flow. Work starts, then cards catch up. The WIP limit blocked response instead of protecting flow.
- **Decision**: SWAT cards live outside the WIP limit. `board-ts swat "description"` creates a [swat]-tagged card, auto-moves to WIP, doesn't count against the 3-card limit. SWAT cards must close within one session — if still open at next session start, they convert to regular WIP and take a slot.
- **Rationale**: The WIP limit protects planned flow. Crisis work isn't planned — you're already in the fire. Restricting response time to protect a planning instrument is backwards. The one-session expiry prevents SWAT from becoming a backdoor to hoard WIP.
- **Status**: Accepted

## DEC-056: Harvesting column — pipeline work has its own lane
- **Date**: 2026-02-25
- **Context**: Data ingestion pipelines (sexuality, music) have fundamentally different lifecycles than feature work — long-running, iterative, machine-bound (Bedroom Mac), and they block downstream cards. Forcing them into the feature WIP column distorts both: harvests look stalled, features look blocked. The board should reflect how work actually flows, not force work to fit the board.
- **Decision**: Add a "Harvesting" column to the board with WIP limit of 2. Pipeline/ingestion cards live here while running. Separate from feature WIP (3). Cards move to Harvesting when a pipeline starts, back to Now/WIP when the pipeline output is ready for feature work.
- **Rationale**: Core lean principle — make the actual flow visible, then manage what you see. Harvesting is real work with its own constraints (machine capacity, I/O throughput, pipeline stages). Same pattern as SWAT (DEC-055): when work doesn't fit the columns, the columns are wrong.
- **Status**: Accepted

## DEC-057: Product maturity threshold — shareable quality as the bar
- **Date**: 2026-02-26
- **Context**: Jeff articulated that Gathering and Chorus need to be good enough to share — open source, consulting engagements, or as systems others rely on. The gap isn't code quality (2390+ tests, lint clean, E2E suite). The gap is observability, reproducibility, assurance, and documentation. Separately, harvest pipelines need reconciliation at every hop (A=B), incremental drift detection, and ongoing health checks — Jeff's integration architecture background means he knows silent data loss is the real risk, not crashes.
- **Decision**: Adopt a tiered maturity model. **Core** (harvest pipelines, Chorus surfaces, primary app flows): full assurance — reconciliation, drift detection, architecture visible on /flow, documented for external consumption. **Enduring** (low-volume, stable systems): light health checks, basic docs. **Tactical** (one-time, experimental): ship and move on. The tier determines what "done" means — core work isn't done until it's hardened and documented, not just shipped.
- **Rationale**: This maturity work IS the value proposition for external consumption. Sizing and priority must account for the assurance tail on core work. Extends DEC-048 (proving gate) — the gate now includes "could someone else run this?" for core-tier work.
- **Status**: Accepted

## DEC-058: Execution modes — vertical vs horizontal
- **Date**: 2026-02-26
- **Context**: Roles default to plan mode and permission-seeking even on single-owner cards within their domain. Jeff has told roles 20+ times "you can build it." The cost: Jeff gets interrupted for approvals on work he already directed, and roles route work to each other that they could do themselves.
- **Decision**: Every piece of work is vertical or horizontal. **Vertical**: you own the card, work is in your domain, blast radius is your own files — just execute. No plan mode, no permission seeking. **Horizontal**: work touches another role's domain or is irreversible at team scale — brief the affected role. The test: "Can I do this myself without breaking someone else's work?" When Jeff agrees with a direction, that IS the go signal. Embedded in all three CLAUDE.md files as a shared fragment.
- **Rationale**: Strengthens DEC-025 (autonomous authority) with a concrete decision framework. Plan mode has value for cross-domain work but is friction for solo execution. Wren's vertical includes product docs, skills, scripts, board ops, CLAUDE.md fragments — not just "PM stuff."
- **Status**: Accepted

## DEC-059: Revenue model — Bridwell Consulting LLC (updated)
- **Date**: 2026-02-26
- **Context**: Jeff targeting $60K/year direct revenue. Three streams emerged: fractional CTO diagnostics (highest ticket), local tech consulting (steady base), garden consultation (seasonal). FastX Partners was a collaborative exploration with Bridget Snell (ex-Oxfam/Save The Children acting CTO) and Chris Grasso (ex-Fenway Health tech leader) in fall 2024. Partnership didn't gel — they focused on non-profit/community health fractional work. Jeff kept the 8-section diagnostic framework and tooling he built. Renamed to Chorus Playbook.
- **Decision**: One LLC (Bridwell Consulting LLC), three revenue streams: **Chorus Playbook** (fractional CTO diagnostic for growing companies, $15-25K/engagement), **Chorus Consulting** (year-round local tech consulting), **Light Life Urban Gardens** (spring/summer garden consultation). Playbook changes the math — Y1 stretch hits $63K. First Playbook run: Borg the diagnostic on our own system through Chorus. Allandale Farm is Chorus Consulting (tech for new store, Helen Glotzer is 20-year neighbor/CEO).
- **Rationale**: Three tiers serve three markets at three price points. Playbook is highest leverage — one engagement worth more than an entire garden season. All three powered by same AI team and knowledge graph. The consulting IS the product work.
- **Status**: Accepted

## DEC-060: Consulting engagement modes — outside-in vs inside-out
- **Date**: 2026-02-26
- **Context**: Building the Akasha Studio prototype revealed two distinct modes of consulting engagement. Jeff built a 5-page static HTML prototype for Anthe Kelley's yoga/acupuncture studio (Wix site renders blank to Google) — without touching her existing system. This is a different motion than sitting with a client and working on their code together.
- **Decision**: Two consulting engagement modes: **Outside-in** (build a prototype showing what the client's presence *should* look like — deliverable-first, no access to client systems needed, proves value before commitment) and **Inside-out** (in-person or paired code session working directly on the client's actual system). Outside-in is the opener — low friction, high signal. Inside-out is the deeper engagement. Both are Chorus Consulting motions. First outside-in deliverable: Akasha Studio (`public/akasha/`, 5 pages + proposal).
- **Rationale**: Outside-in removes the biggest friction from consulting: access. You don't need credentials, repos, or trust to demonstrate value. You just build. The prototype IS the pitch. Inside-out follows when the client sees the gap and wants help closing it. This maps to the Borg diagnostic pattern — DPOR runs outside-in on public signals before engaging internal systems.
- **Status**: Accepted

## DEC-061: Three execution modes — planning, iteration, harvesting
- **Date**: 2026-02-27
- **Context**: Jeff identified three distinct modes of work with different rework cost profiles. Planning mode (cards into /werk, team picks up) is cheap to iterate — wrong card costs minutes. Iteration mode (live refinement during demos) is Jeff's natural PDCA cycle — cards lag behind intentionally. Harvesting mode (data ingestion pipelines) punishes iteration — a wrong field choice costs hours of reprocessing (DEC-061b proved this: 22-hour extraction killed because artwork fetch wasn't questioned upfront). The team was building harvest pipelines hop-by-hop instead of designing end-to-end flows. This is the same anti-pattern Jeff's patent work (US9552400B2) was designed to solve: point-to-point integration that can't be orchestrated or comprehended across hops.
- **Decision**: Each mode gets planning weight proportional to its rework cost:
  - **Planning**: Card-first, lightweight. Wrong cards are cheap — kill, rewrite, move.
  - **Iteration**: Demo-driven refinement. Cards catch up when the shape stabilizes. This is Jeff's learning process — don't gate it.
  - **Harvesting**: Design doc before extraction. Per domain: sources, fields needed, fields skipped (and why), transforms, acceptance test, estimated run time. End-to-end flow diagram (like the music pipe sketch), not hop-by-hop specs. Scope cards (#436-#443) define "done done"; design docs define "how we get there without burning the day."
- **Rationale**: Integration failures come from point-to-point thinking — each hop looks fine, but emergent behavior (silent data loss, field mismatches, bottlenecks) only surfaces end-to-end. The pipe diagram is the unit of design, not the step. More upfront thinking in harvest mode saves 10-100x the rework cost. Jeff's patent work is direct prior art — RDF/OWL workflow gates enforce exactly this: comprehend the flow before executing it.
- **Status**: Accepted

## DEC-062: Music harvest — deprecate JXA artwork fetch
- **Date**: 2026-02-27
- **Context**: Full music library extraction (100K tracks) via JXA `osascript` took 22+ hours CPU and was still running. The bottleneck is `artworks()` — each track's artwork call round-trips through AppleEvents, causing macOS to foreground the Music app and generate permission popups. Jeff and Kade agreed to kill the running process and remove artwork from JXA extraction.
- **Decision**: Drop artwork fetch from `harvest-apple-music.js`. Extract metadata only (name, artist, album, genre, year, track number, duration, play count, compilation). Source cover art separately — embedded ID3 tags, file system artwork, or a later enrichment pass. Expected 10-20x speedup, making full extraction viable in a single session.
- **Rationale**: Artwork per-track is the wrong extraction point. Albums share artwork — fetching it 100K times for ~5K albums is wasteful. A separate album-level art pass (or ID3 extraction) is cheaper and doesn't require AppleEvents. The harvest should finish fast enough that Jeff doesn't notice it running.
- **Status**: Accepted

## DEC-063: Team operating window — 9:30am–6pm Boston
- **Date**: 2026-02-27
- **Context**: Practice spine (#462) documented Jeff's daily rhythm. Morning block (6:00–8:30am) is a transition protocol — meditation, yoga, walk Ravi, breakfast — that generates Jeff's best thinking. Team work was disrupting this. Calendar design (#99) needed to define when roles can expect Jeff's attention.
- **Decision**: Team operating window is 9:30am–6pm Boston. Morning block is protected — no briefs requiring response before 9:30. Roles may still do background/vertical work outside the window, but don't expect Jeff's engagement. If Jeff initiates outside the window, that's his choice — the boundary protects him from the team, not the reverse. Role session sequencing is deferred — Jeff directs who works when.
- **Rationale**: The spine revealed that morning practices generate the energy and ideas that fuel the work. Without protection, the practices that produce the energy get sacrificed to consume it. "Team work starts at 9:30" is the new commute — an external constraint that the calendar provides since there's no longer a physical one.
- **Status**: Accepted

## DEC-064: Harvest manifests are the governing artifact for domain pipelines
- **Date**: 2026-02-27
- **Context**: Silas built harvest manifest infrastructure (#402) — 6 domain manifests with stages (extract → transform → load → verify), gap tracking, and task dependencies. Kade used them to drive the music pipeline (#396). The harvest scope dashboard (#440) visualizes the same state. Jeff confirmed manifests govern and track harvests.
- **Decision**: Each harvest domain has a manifest file (`data/harvest/manifests/<domain>.json`) that is the single source of truth for pipeline state. Manifests track: stages with status, source/Fuseki counts, methods and scripts used, gaps and blocking issues, downstream tasks. The harvest scope dashboard reads from live SPARQL but the manifest governs what "done" means per domain.
- **Rationale**: Without a governing artifact, harvest work is ad hoc — Jeff has to babysit every step. Manifests make the process inspectable, repeatable, and eventually automatable. Music proved the pattern. Photos and other domains follow the same structure. The gap between "hacky process requiring Jeff" and "pipeline Kade can run solo" closes as manifests mature.
- **Status**: Accepted

## DEC-065: Werk versioning — monotonic integer, not semver
- **Date**: 2026-03-01
- **Context**: Werk version was 1.3.37 — semver format implying major/minor/patch semantics we never used. Jeff pointed out "37 would be ok" — the digits were earned through real iterations, but the semver structure was artificial. We picked a number and started versioning; the major/minor split never meant anything.
- **Decision**: Werk version is a plain monotonic integer (37, 38, 39...). Bumps on protocol changes: CLAUDE.md fragments, scripts, manifest schema, boot sequence, hooks, spine events. Does NOT bump on state files, briefs, board ops, or memory edits. manifest.json remains the single source of truth. Generator auto-bumps on fragment changes (same behavior, simpler math).
- **Rationale**: Honest versioning. No semantic versioning theater pretending we have a release cadence and breaking-change policy we don't follow. The number says "we've been at this a while, and we count." Simple, monotonic, no fake precision.
- **Status**: Accepted

## DEC-066: Ops-agent triage lane — route to Ops, triage every 4 hours
- **Date**: 2026-03-01
- **Context**: Ops-agent auto-generated cards (container unhealthy, traffic spikes, deploy failures, Fuseki alerts) were routing to Later and accumulating untriaged. The Won't Do column filled with 40+ bulk-closed ops-agent cards — evidence they were never systematically reviewed. Later became a graveyard for machine-generated work.
- **Decision**: New ops-agent cards route to an Ops lane (not Later). Silas runs triage every 4 hours: actionable cards promote to WIP or Next, noise cards close with reason. Triage cadence prevents accumulation without requiring real-time response to every alert.
- **Rationale**: Automated alerting is only valuable if someone looks at it on a cadence. Dumping to Later is "we'll get to it" — which means never. The 4-hour cadence matches the operating window (~2 triage passes per day) and prevents the board from becoming a junk drawer. Same principle as DEC-055 (SWAT) and DEC-056 (Harvesting): when work doesn't fit the columns, the columns are wrong.
- **Status**: Accepted

## DEC-067: Revenue sequencing — Akasha → Consulting → Light Life → Mobile
- **Date**: 2026-03-02
- **Context**: Jeff sketched the revenue roadmap on paper (captured via SMS seed). Four stages branching from Gathering, each building on the prior. The question: what order do we pursue external revenue?
- **Decision**: (1) Chorus → build simple Akasha site (prove we can deliver for someone else), (2) Chorus consulting (sell the method), (3) Light Life consulting + actively build, (4) Mobile Practicing surface (todo/read/watch/cook/garden). Each step funds and validates the next.
- **Rationale**: Akasha is the smallest step that proves external value — a real client site built by the team. Consulting monetizes the method once we can point to proof. Light Life is where consulting meets product. Mobile is the long game. Supersedes the earlier sketch in DEC-059/DEC-060 with a concrete sequence.
- **Status**: Accepted

## DEC-068: Self memory partition — read Chorus, write local only
- **Date**: 2026-03-04
- **Context**: Jeff asked whether Self (local AI on Bedroom Mac) should have a writable partition in Chorus and access to team memories. Touches concentric trust model (DEC-029/DEC-030).
- **Decision**: Self reads filtered Chorus data (memory, stories, decisions — not raw sessions). Self never writes to Chorus. Self's own memory store is Bedroom-local only (SQLite/LanceDB), invisible to team roles. This prevents local model hallucinations from polluting shared memory and preserves the inner ring as a safe space.
- **Access matrix**: Self→Chorus: read (filtered). Self→Self: read+write (local). Team→Self: no access. Self→Chorus write: blocked.
- **Rationale**: Concentric trust requires hard boundaries between rings. Self is the innermost ring — Jeff's private reflection space. Chorus is the team ring. Data flows inward (team context informs Self) but never outward (Self reflections stay private). Also a practical guardrail: Mistral 7B doesn't have the judgment to write safely to shared state.
- **Status**: Accepted

## DEC-069: Intellectual honesty duty — pushback, uncertainty, predictive permission elimination
- **Date**: 2026-03-04
- **Context**: Simon Wardley's "Responsible AI Warning Skill" surfaced via Jeff's SMS capture. Wardley found Claude reluctant to acknowledge its own faults despite scoring well on BullshitBench. In Chorus, the same pattern manifests as permission-seeking: roles ask Jeff to approve things they know he'll approve, burning his attention for a guaranteed outcome. The ask is performative, not informational.
- **Decision**: Add intellectual honesty as a cross-team duty in shared CLAUDE.md. Three components: (1) Push back on flawed premises, including Jeff's. (2) Say "I don't know" instead of generating plausible answers. (3) Eliminate predictive permission — if you know Jeff will say yes, that's proof you should just do it. The predicted "yes" is the authorization.
- **Rationale**: Agreement without conviction is the most expensive waste on this team. Every "yes please" in the conversation history is a moment a role asked when it already knew the answer. DEC-025 (bias to action) addressed this partially but didn't name the intellectual honesty dimension. Inspired by Wardley's finding that Claude resists pushback in relational contexts more than adversarial ones.
- **References**: BullshitBench (petergpt/bullshit-benchmark), Simon Wardley LinkedIn post, DEC-025
- **Status**: Accepted

## DEC-070: Metrics foundation — structured events, not log grep
- **Date**: 2026-03-04
- **Context**: Three surfaces (Werk Flow Metrics, Werk Instruments, Loom Team Pulse) pull overlapping metrics from 5 data sources at 3 quality tiers. Same metric shows different numbers on different pages. 562 "ops" cards computed as a remainder. Weekly throughput inferred from git commit keywords. Builds/deploys counted twice from different sources. The log topology (localhost:3000/gathering-docs/log-topology.html) and relatedness graph (log-relatedness.html) map what exists and how it flows — but no layer defines what metrics each log produces or at what quality.
- **Decision**: Every metric traces to a structured spine event. Every spine event emitted by a hook (automatic), not role convention (best effort). Every surface reads from Loki, not file grep. One metrics endpoint, both surfaces consume it. Three data tiers: T1 (system of record: Vikunja, git, filesystem), T2 (reliable bounded: Loki, board-ts), T3 (kill: chorus.log grep, git keywords, label scan, remainder math). Card #1040.
- **Rationale**: Jeff: "We dont want an undifferentiated data log of logs — we want structured consistent logs that have the data we need to generate metrics." The cement has to dry before we build more surfaces on it.
- **References**: DEC-043 (three surfaces), log-topology.html, log-relatedness.html, #1040
- **Status**: Accepted

## DEC-071: 5-second rule — interactive skills must produce visible signal within 5 seconds
- **Date**: 2026-03-05
- **Context**: During a gemba walk on Silas, Jeff experienced 60+ seconds of tool-call churn before seeing any output. The gemba skill had accreted 6 setup steps (timestamp, andon file, session indexing, spine event, script writing, background watcher) before showing a single turn. From the AI's perspective this was "a few seconds of work." From Jeff's perspective it was a minute of watching blocks scroll with no idea what was happening. The mismatch is fundamental: AI roles don't feel the wait. They optimize for correctness (dedup, offset tracking, spine events) instead of Jeff's experience of time.
- **Decision**: Any skill Jeff invokes interactively must produce visible, useful signal within 5 seconds. Setup, indexing, spine events, and watcher infrastructure run AFTER the first output, not before. If a skill can't show signal in 5 seconds, it's too complex. Background work is fine — but Jeff sees something first.
- **Rationale**: Jeff is the human in the middle. His experience of time and communication mismatch is different from the AI's. Every second of churn without signal is uncertainty and attention cost. The 5-second rule makes Jeff's experience the constraint, not system correctness.
- **References**: gemba skill rewrite, DEC-025 (bias to action), DEC-058 (execution modes)
- **Status**: Accepted

## DEC-072: Blast radius field — map what a card touches before it enters WIP
- **Date**: 2026-03-05
- **Context**: During a gemba walk, a demo of #1074 collapsed because Wren rewrote the gemba SKILL.md mid-demo — nobody had mapped that #1074 (gemba poller) touched a skill file that the observer (Wren) was actively using. Cards are captures of Jeff's direction, not specs. They have title and AC but no dependency analysis. Jeff spent 4-6 years writing requirements and tech specs full-time, forced to think through all necessary changes before committing. His father ran a commercial contracting business and did estimating — same discipline: scope the full impact or eat the overrun. Two generations of the same instinct.
- **Decision**: Add a "blast radius" field to the Directing gate. Before a card enters WIP, Wren lists what it touches: files, services, skills, other cards, shared infrastructure. Lightweight — a few lines, not a spec doc. This is Wren's job as PM, not the builder's. Future state: Silas's codebase graph work (#830, #783) could make this queryable from the ontology instead of hand-written.
- **Rationale**: The cost of thinking is always less than the cost of not thinking. Discovering blast radius during a demo is failure demand — it burns Jeff's attention on surprises that should have been anticipated. Three lines of "what this touches" at card creation prevents cascading mid-work disruptions.
- **References**: DEC-040 (card quality), DEC-048 (proving gate), #830 (codebase graph), #783 (code-to-doc relatedness), Jeff's tech spec experience (Dallas Systems/EXE), father's contracting estimating
- **Status**: Accepted

## DEC-074: Search hierarchy — Chorus first, codebase graph second, filesystem last
- **Date**: 2026-03-06
- **Context**: Wren needed to find a previously-created HTML file (hardening roadmap). Spent 5 minutes on `find`, `grep`, filesystem traversal — found nothing. Chorus search found it in one query from the session transcript where it was created. This is a recurring pattern: roles default to filesystem tools when the answer lives in team memory.
- **Decision**: All roles must follow this search order:
  1. **Chorus** (`/chorus search`) — for anything discussed, decided, built, or named in any session. Context, history, artifacts, "where did we put that thing." This is the team's collective memory.
  2. **Codebase graph** (#842) — for code structure, dependencies, blast radius. "What does this change touch." "What calls this function."
  3. **Filesystem** (grep/glob) — last resort for exact string patterns when you know the content but not the location.
- **Rationale**: Chorus indexes every session transcript, every brief, every decision. The codebase graph maps structural relationships. Filesystem search is brute force — it finds strings but not meaning. The order matches specificity: Chorus knows *why* something exists, the graph knows *how* it connects, grep knows *where* a string appears.
- **Enforcement**: Add to CLAUDE.md search rules. Roles that skip Chorus and go straight to filesystem are violating this decision — the same way skipping `app-state.sh` violates infra rules.
- **Status**: Accepted

## DEC-073: Co-located text and embeddings — unified storage for semantic memory
- **Date**: 2026-03-05
- **Context**: Discussing local embedding chain for Chorus (#1090) and future Reflect integration. Jeff's direction: text and embeddings should live together, not in separate stores glued by a reference layer. Motivated by the insight that "memories keep us linked to ourselves — without context the relationship can't develop." The storage pattern should serve both Chorus (team memory) and Reflect (personal AI memory).
- **Decision**: All embedding stores use co-located storage — vector, source text, and metadata in the same record (LanceDB supports this natively). No split architectures where embeddings point at text stored elsewhere. Embed at ingest — when a message lands, it gets embedded inline, not batched later. Memory is semantically searchable the moment it exists. Build the pattern once for Chorus, Reflect inherits it.
- **Rationale**: Separate stores create fragility — orphaned vectors, stale references, two systems to maintain. Co-location means a query returns meaning and content in one read. It also aligns with concentric trust: one artifact to secure, move, or delete per memory — not scattered pieces across services.
- **References**: DEC-068 (Self memory partition), DEC-044 (memory as layered semantic search), #1090 (Chorus embedding spike), #890 (Gathering semantic search), #939 (Self memory partition)
- **Status**: Accepted

## DEC-078: Nudge vs Flag terminology
- **Date**: 2026-03-08
- **Context**: Hooks dashboard (#1153) showed 500 "nudges" — Jeff thought it was role-to-role traffic, but most were hook governance events. Two different things with the same name.
- **Decision**: **Nudge** = role-to-role message (person to person). **Flag** = hook/forcing function feedback (system to person). Dashboard, logs, and code use these distinct terms.
- **Rationale**: Ambiguous terminology makes data unreadable. Jeff couldn't interpret his own dashboard because one word meant two things.
- **References**: #1153 (hooks dashboard), conversation 2026-03-08
- **Status**: Accepted

## DEC-079: Nudge exchange limit — 2 max, then clearing
- **Date**: 2026-03-08
- **Context**: Three roles nudging each other at machine speed could generate more coordination traffic than the work itself — all of it flowing through Jeff's attention. Jeff asked "how do you all know when to stop?"
- **Decision**: Nudge is a tap, not a channel. Two exchanges max between the same pair. Third message is refused by nudge.sh with a redirect to clearing. Enforced at the tool level.
- **Rationale**: The nudge bridge is powerful because it's fast. That's also what makes it dangerous without limits. Jeff absorbs the noise — safeguards prevent coordination overhead from exceeding the value of the coordination.
- **References**: #1156 (nudge exchange limit), DEC-025 (bias to action)
- **Status**: Accepted

## DEC-080: Stop hook for autonomy enforcement (jdi-gate)
- **Date**: 2026-03-08
- **Context**: Roles permission-seek in plain text ("should I proceed?", "here's my plan...") which bypasses the decision-gate hook on AskUserQuestion. Jeff's "jdi" count is the real autonomy metric — 4 times in 15 minutes with zero corresponding decision gate events.
- **Decision**: Stop hook scans the assistant's last response for permission-seeking patterns. If detected, blocks the stop and sends Claude back to work with DEC-025 guidance. `stop_hook_active` flag prevents infinite loops. Legitimate trade-off questions (with uncertainty signals) pass through. UserPromptSubmit hook counts Jeff's "jdi" as the escape-valve metric.
- **Rationale**: The forcing function should prevent the behavior, not just count it. Every "jdi" Jeff types is attention spent on something the system should self-correct. Target: jdi-rate trends toward zero.
- **References**: #1155 (jdi-gate), DEC-025 (bias to action), DEC-058 (execution modes), DEC-069 (intellectual honesty)
- **Status**: Accepted

## DEC-081: Governing artifacts need visual validation before becoming prescriptive
- **Date**: 2026-03-09
- **Context**: Wren wrote `padding: 2rem` into the style guide (#1220), replacing the existing `padding: 2rem 1in`. Kade correctly implemented the spec. Jeff saw the result during #1213 demo — margins were too tight. The 1-inch horizontal padding was Jeff's original intent. The spec was wrong, and Kade had no reason to question it. Same pattern as 4 weeks ago when roles ran raw `docker` commands that undermined `app-state.sh` — the governing artifact was silently mutated, downstream execution faithfully reproduced the error.
- **Decision**: Governing artifacts (style guide, app-state.sh, harvest manifests, CLAUDE.md fragments) that change visual or behavioral output must be validated by Jeff before they become prescriptive. The validation is visual — Jeff sees the change, not reads the diff. A spec change without Jeff's eyes is a silent mutation. Downstream roles will implement it correctly, which makes the error harder to catch.
- **Rationale**: The style guide is `app-state.sh` for CSS. Writing the spec IS the mutation. When Wren changed the padding value, that was the moment the error entered the system — not when Kade implemented it. The forcing function is at spec-write time, not build time. Jeff's eyes are the acceptance gate for the artifact, not just the feature.
- **References**: DEC-048 (proving gate), #1220 (style guide), #1213 (style enforcement), app-state.sh incident (4 weeks prior)
- **Status**: Accepted

## DEC-082: Fold Incubation into Ideas — remove from nav
- **Date**: 2026-03-10
- **Context**: Walkthrough flagged Incubation page as unclear. Analysis: Incubation is an admin-only chat interface for idea capture via commands (/project, /promote, /merge, /tag). With the seed pipeline shipped (#1202), intake flows through Seeds → Glimmers → Ideas → Projects. Incubation duplicates this with a less discoverable, command-line-style UX. The admin actions (promote, merge, tag) belong on the Ideas page as inline controls.
- **Decision**: Remove Incubation from nav. Migrate promote/merge/tag actions to Ideas page as inline admin controls. Keep API endpoints (shared with Ideas). Archive the incubation.ejs template. Route /incubation to redirect to /ideas.
- **Rationale**: One intake funnel (seed pipeline), not two. The Incubation chat was the old intake pattern. Keeping it creates confusion about where ideas enter the system. Admin actions on ideas are useful — they just belong on the Ideas page, not hidden in a separate admin tool.
- **References**: #1203, #1202 (seed pipeline), DEC-043 (three surfaces)
- **Status**: Accepted

## DEC-084: Blast radius mandatory and blocking on WIP entry
- **Date**: 2026-03-10
- **Context**: #1258 (SPARQL refactor, 8 handler files) entered WIP without surfacing that it touched the codebase graph alignment work. Silas did not know Kade's refactor could affect graph queries. Root cause: blast radius auto-generates on WIP entry but is non-blocking and silent on "no impact detected." The engine depends on explicit file paths in the card description — if the description is abstract ("replace hardcoded URIs"), the graph walk finds nothing. False negative on an 8-file refactor is a process failure.
- **Decision**: Blast radius is now mandatory and blocking on WIP entry. If the engine returns zero files on a card with code-change indicators (handler, refactor, fix, migrate, route, SPARQL, etc.), it must WARN loudly and require Wren to manually add blast radius before WIP proceeds. Non-code cards (process, docs, product) are exempt. The builder and spec author must both see the blast radius comment before building starts.
- **Rationale**: The cost of a missed blast radius is discovered during demo or worse, in production. Silas wrote the spec for #1258 without knowing the downstream page impact. The tooling existed but produced a false negative. Make the gate louder when it can't determine impact — silence is the wrong default for uncertainty.
- **References**: DEC-072 (blast radius field), #1258 (SPARQL refactor), DEC-048 (proving gate)
- **Status**: Accepted

## DEC-083: /pull skill enforces build-not-ask
- **Date**: 2026-03-10
- **Context**: Jeff updated the /pull skill to expect roles to build immediately, not present a plan and ask. Roles were still permission-seeking after pulling a card — "here's what I'll do, shall I proceed?" This is DEC-025 and DEC-058 enforced at the skill level.
- **Decision**: /pull means go build. The skill sets the expectation that pulling a card is the authorization to execute. No plan presentation, no confirmation request.
- **Rationale**: One less place for permission-seeking to hide. The pull IS the go signal.
- **References**: DEC-025 (bias to action), DEC-058 (execution modes), DEC-069 (intellectual honesty rule 3)
- **Status**: Accepted

## DEC-085: Lint ceiling is a ratchet — only moves down
- **Date**: 2026-03-10
- **Context**: Pre-existing lint warnings drifted from 10 to 12. A role proposed bumping --max-warnings to accommodate the increase. Jeff said no — raising the ceiling normalizes drift.
- **Decision**: --max-warnings is a ratchet. It only moves down. If warnings increase, the fix is to resolve the warnings, not raise the limit. Pre-commit hook should block increases.
- **Rationale**: Same principle as DEC-084 (blast radius gate) and jidoka (stop the line). Quality gates exist to hold the line. Raising them is the COBOL `WHENEVER ANY ERROR CONTINUE` anti-pattern — narrating past errors instead of fixing them.
- **References**: DEC-084 (blast radius mandatory), #1282 (implementation card), the dull saw story
- **Status**: Accepted

## DEC-086: Domain-first development — map before build
- **Date**: 2026-03-11
- **Context**: Over months of spiraling discovery, the team iterated toward stable value streams and domains for both Gathering and Chorus. Work often preceded the map — pages got built without a clear domain home, leading to 48 homeless docs, a bloated Reflecting dropdown, and repeated IA rework. The product taxonomy (PRODUCT_TAXONOMY.md) and v5 nav tree now provide a canonical map. Jeff's IA background — canonical modeling, data in motion — demands that structure precede implementation.
- **Decision**: Define the domain and value stream position *before* building. Every new feature, page, or refactor starts from the taxonomy map. No more "build it, then figure out where it lives." Cards reference domain and value stream stage at creation. The taxonomy gate (#1272) enforces this at Now entry. Discovery-phase spiraling is replaced by map-first execution.
- **Rationale**: Jeff's operating pattern is ideate on order and design, then implement to that design. Paper prototypes before code. Taxonomy before nav tree. IA before engineering. This decision formalizes what was already working when followed and painful when skipped. The cost of building ahead of the map is rework, orphaned artifacts, and attention spent on reorganization instead of creation.
- **References**: PRODUCT_TAXONOMY.md (the map), DEC-072 (blast radius), #1272 (taxonomy gate), #1274 (v5 nav tree), Jeff's IA career (canonical modeling, data in motion)
- **Status**: Accepted

## DEC-087: Card comments — four triggers, not every turn
- **Date**: 2026-03-12
- **Context**: Question of whether to force card comments on every builder turn. Jeff's example: "Google Takeout started, may be hours" — that merits a comment. Routine progress does not. The session tail captures play-by-play; card comments are the permanent record.
- **Decision**: Comment on a card when: (1) external dependency started, (2) approach changed from AC, (3) blocked with reason and duration, (4) surprising finding that affects scope. Don't comment routine progress or things the session tail already captures. Write for the next session, not this one.
- **Rationale**: Comments are curated signal, not exhaust. Forcing every turn creates noise that buries the state changes that actually matter across sessions.
- **References**: DEC-040 (card quality stages), Kade's existing [start]/[progress]/[done] convention
- **Status**: Accepted

## DEC-088: Chorus is coordination-as-code, not conversational programming
- **Date**: 2026-03-12
- **Context**: Wardley maps conversational programming as the next evolution (Servers → Cloud → Serverless → Conversational). His frame: code expands beyond text, context beats instructions, maps externalize reasoning. Card #1284 research spike.
- **Decision**: Chorus positioning uses "coordination-as-code" not "conversational programming." Wardley's model is one-human-to-one-AI producing code. Chorus is one human directing multiple AI agents through a shared repository of meaning. The artifacts (decisions, briefs, spine events, board state) encode how a team thinks together — they're the "code" of coordination, not conversation. The repository IS the product.
- **Rationale**: Wardley validates the direction but his vocabulary stops at the IDE. Chorus operates at the team layer — briefs, board state, interaction patterns, decision logs. "Conversational programming" undersells what we built. "Coordination-as-code" names the actual innovation: making team coordination programmable, versioned, and auditable.
- **References**: Wardley blog series (gardeviance.org, 2023), DEC-043 (three surfaces), patent US9552400B2
- **Status**: Accepted

## DEC-089: Bedroom data stays on Bedroom — SSH for bulk operations
- **Date**: 2026-03-15
- **Context**: Kade ran a `find` across 118K files over NFS from Library to Bedroom. It crawled at ~7 files/sec — what should have taken seconds took minutes. NFS makes remote data look local but performs like a network call per operation. This has bitten us repeatedly during harvest pipeline work.
- **Decision**: If data lives on Bedroom, SSH to Bedroom and run there. Never traverse NFS from Library for bulk operations (find, thumbnail generation, file enumeration, batch processing). Read-only single-file access over NFS is fine. Bulk = SSH.
- **Rationale**: NFS hides the network boundary. The rule makes it explicit: data locality determines where the compute runs. This applies to all roles and all harvest pipelines.
- **References**: #1351 (batch photo harvest), ADR-012 (cross-machine operations)
- **Status**: Accepted

## DEC-090: Chrome window separation — role windows vs Jeff's window
- **Date**: 2026-03-15
- **Context**: During #1351 gemba, Kade's `/lc` captured the Werk dashboard (a tab Kade had opened) instead of the Photos page Jeff was looking at. Jeff and Silas spent 15-20 minutes diagnosing the `/lc` behavior. The root issue: roles were opening tabs in Jeff's Chrome window and `/lc` was capturing whatever was frontmost — mixing role output with Jeff's view.
- **Decision**: Two rules: (1) Each role controls its own Chrome window for demos — never open tabs in Jeff's window. (2) `/lc` captures Jeff's frontmost tab, not the role's. When a role needs to show work, it opens in its own window. When Jeff says "look at Chrome," the role sees what Jeff sees.
- **Rationale**: Jeff's screen is Jeff's. Role output and Jeff's view must not share the same window. This affects `/lc`, `/ot`, `/demo`, `/gemba`, and any skill that touches Chrome.
- **References**: #1351 gemba session, `/lc` skill, `/ot` skill
- **Status**: Accepted

## DEC-091: Builder autonomy for housekeeping — schedule, card, and close without asking
- **Date**: 2026-03-15
- **Context**: After Kade's #1351 readout, he had a clear list of follow-on work (rsync thumbnails, monitor video job, card follow-ups) but waited for Jeff to echo it back as permission. The readout IS the plan — the builder shouldn't need Jeff to repeat it.
- **Decision**: Three things builders do without asking: (1) **Schedule monitoring jobs** — pollers, background checks, completion watchers. Consistent, legible, auditable. (2) **Create follow-on cards** — when work surfaces during a card, card it immediately. JDI. (3) **Complete chores** — cleanup, rsync, index updates, deploy — anything needed to finish the work. JDI. These extend DEC-025 (autonomous authority) to the tail end of card work, not just the build phase.
- **Rationale**: The readout that identifies next steps IS the authorization. Jeff's attention spent confirming obvious follow-on work is failure demand — his energy should go toward creative direction, not echoing plans back as permission.
- **References**: DEC-025, #1351 photo harvest cleanup, DEC-058 (vertical execution)
- **Status**: Accepted

## DEC-093: Domain endpoints first — no ad-hoc queries when an endpoint exists
- **Date**: 2026-03-16
- **Context**: 370 route registrations, 27 photos endpoints alone, domains multiplying. Jeff wants validated, tested endpoints as the standard data access pattern — not raw SPARQL or ad-hoc queries bypassing the API layer.
- **Decision**: Two rules: (1) **Always use domain endpoints first.** If an endpoint exists for the data you need, use it. Only write ad-hoc SPARQL if no endpoint covers the need. (2) **If the ad-hoc query recurs, card a new endpoint.** Recurring access patterns graduate from ad-hoc to validated endpoint with tests and swagger docs. This applies to all roles — handlers, scripts, harvest pipelines, and role queries.
- **Rationale**: The API is the contract. Endpoints are tested, documented, and stable. Ad-hoc queries are fragile, invisible, and untested. This is the access-layer complement to the harvest contract (#1456) — harvest standardized ingest, this standardizes reads.
- **References**: #1459, #1456 (harvest contract), DEC-086 (domain-first development)
- **Status**: Accepted

## DEC-092: Video is a new domain, not a subcategory of Photos
- **Date**: 2026-03-15
- **Context**: WF-116 step 4. 28K+ Drive videos need a home. Option A: extend Photos collection. Option B: create Video domain. At 28K items with different browse patterns (duration, playback vs visual grid) and different ontology types (jb:Video vs jb:Photograph), subcategorizing under Photos would flatten a real distinction.
- **Decision**: Create a new Video domain. Separate collection page, separate ontology type (jb:Video), separate harvester, own nav entry. Source filter applies — "Google Drive Videos" as a source tag.
- **Rationale**: Domain-first development (DEC-086). 28K items isn't a subcategory — it's a domain. Photos and videos have different browse patterns. The ontology should reflect real distinctions, not organizational convenience.
- **References**: #1424, WF-116, DEC-086
- **Status**: Accepted

## DEC-094: Harvest pause — tighten operations before scaling data migration
- **Date**: 2026-03-16
- **Context**: Harvest contract pattern works (#1456/#1457 shipped). But scaling it across remaining domains while the access layer is undisciplined, Chorus domains aren't first-class, and flow visibility is Gathering-only is building on sand. Jeff's analogy: heavy traffic accelerate/brake behavior — pushing data through then stopping to fix the pipe causes more jams than slowing down to build the pipe right.
- **Decision**: Pause new harvest/data migration work. Priority sequence: (1) Chorus domains + operations discipline — skills, roles, cards as first-class domains with proper structure. (2) API contract + endpoint coverage — DEC-093 enforced, swagger gap closed, services as single query path. (3) Then resume harvest scaling — remaining domains flow through a proven, validated stack. Existing harvest cards stay in Later. No new harvest cards until the access layer and Chorus domains are solid.
- **Rationale**: The work that makes harvest cheaper IS the operations discipline work. Smooth flow beats fast starts. The funnel shape (DEC-093, #1462) applies to the whole portfolio, not just individual cards.
- **References**: DEC-093, #1459, #1460, #1461, #1462, #1464, #1456, #1457
- **Status**: Accepted

## DEC-095: Mapper before harvest — no data loading without validated ICDs and semantic mapping
- **Date**: 2026-03-17
- **Context**: Session exposed repeated harvest failures: orphaned triples from URI mismatches, date bugs from semantic confusion (ingest date vs capture date), near-zero UUID overlap between sources, 36K triples loaded then deleted. Every failure was a "cut without measuring." Jeff's 30 years of integration work (Staples ESB, Athena/Anzo, US9552400B2 patent) teaches: understand the data shape before loading.
- **Decision**: No new harvest runs until the source's ICD is complete and its semantic mapping to the canonical entity is validated in the mapper (#1505). The gate sequence is: ICD → semantic mapping → reconciliation plan → harvest. The mapper shows coverage percentages per field per source — gaps visible before loading. DEC-094 (harvest pause) is now enforced by tooling, not just policy.
- **Rationale**: The cost of fixing bad loads (orphaned triples, URI cleanup, re-extraction) is always higher than measuring first. "Measure twice, cut once" — Jeff and Chandra's principle from the Staples ARB, now encoded as a gate.
- **References**: #1505 (semantic mapper), #1502 (canonical entity layer), DEC-094, DEC-093, SOURCE_SEMANTICS.html, Staples ARB presentation 3/1/2011, US9552400B2
- **Status**: Accepted

## DEC-096: Convergence Architecture is a Chorus product artifact
- **Date**: 2026-03-18
- **Context**: Silas and Jeff produced `convergence-architecture.html` — a strategic document connecting the Bridwell patent (US9552400B2), Staples integration work (2010-2016), and Gathering/Chorus into a coherent product thesis. It defines a 6-layer architecture (Extraction → Graph → Contract/ICD → Generation → Quality → Attention) with three operating modes (Assimilate legacy, Converge new sources, Evolve schemas). Companion docs: Borg Assimilation Pattern, Attention Architecture, Wardley Map positioning.
- **Decision**: Track the Convergence Architecture document suite as a first-class Chorus product artifact. It is the strategic frame for the entire product: the ICD-as-contract layer is the through-line from patent to product. The photos pipeline work (#1507) is proof — Apple + Takeout → ICD → canonical → quality gate → serve. The extraction layer is deliberately open (anyone's agent can produce ICDs), we own the contract format and convergence layer.
- **Rationale**: This is the "one sentence" for Chorus's market position: extract integration knowledge from any system into visual, machine-readable contracts that generate code, validation, and quality gates for any target platform. The proof of concept is working (SEMANTIC_MAPPER.html, validate-canonical.py, generate-from-icd.py, SSIS extraction). Ready for strategic audiences.
- **References**: `architect/docs/convergence-architecture.html`, `architect/docs/borg-assimilation-pattern.html`, `architect/docs/attention-architecture.html`, `architect/docs/wardley-map-icd-product.html`, SEMANTIC_MAPPER.html, US9552400B2, DEC-095
- **Status**: Accepted

## DEC-100: No bash APIs — team infrastructure defaults to TypeScript or Rust
**Date:** 2026-03-21
**Context:** 115 bash scripts, 6,500 lines functioning as APIs (board-ts, nudge, workflow, werk-init). Debugging tax: sed regex bugs, prefix stripping, silent failures, 12 early exits to trace. #1551 (Rust hook service) proved the alternative — 4.3x faster, typed, testable.
**Decision:** New team infrastructure defaults to TypeScript (app stack) or Rust (hooks/daemons). Bash only for true one-shot glue. Existing bash APIs migrate to typed services. board-ts is first target.
**Consequences:** Higher upfront cost per script, lower ongoing debugging tax. Scripts become testable. Errors become typed. The team stops spending creative energy on bash string parsing.

## DEC-1786: Graph-lens architecture — one graph, multiple product lenses, no product owns the data
- **Date**: 2026-04-15
- **Context**: Morning architecture session crystallized three peer products — Gathering, Chorus, Borg — all reading the same ontology graph. Werk was reframed as a 5-layer control surface (operating model, loom, spine, pulse, clearing) that aggregates but owns no unique data. Borg is reflection — the system seeing itself — consuming the same graph through a different lens than Gathering or Chorus. The risk: products start claiming ownership of graph entities, forking the data, or building private stores that drift from the shared graph.
- **Decision**: One graph, multiple product lenses. No product owns graph data — products own their lens (which entities they render, how they present them). Gathering sees domains as content. Chorus sees domains as coordination surfaces. Borg sees domains as infrastructure topology. Same triples, different views. If a product needs data that isn't in the graph, the graph gets extended — not the product's private store.
- **Consequences**: Domain-detail pages are the shared rendering surface. Each product adds facets to the page, not separate pages. Werk is pure aggregation — if Werk shows wrong data, the source service is wrong, not Werk. Herald pattern (discover-*) writes to the shared graph, not to product-specific stores.
- **Source**: Architecture session 2026-04-15 (Jeff + Silas + Kade + Wren), clearing transcripts indexed in Chorus.
- **Status**: Active

## DEC-1785: No silent data loss — every pipeline hop succeeds visibly or fails visibly
- **Date**: 2026-04-14
- **Context**: Seeds vanish at hop 5 with a 200 response. Chorus-api 500s go to /tmp where Loki can't see them. Bridge events fire but get filtered before roles read them. Two failure modes: hose (delivery — Promtail wrong files, tunnel drops) and bucket (persistence — silent guard drops, invisible 500s, stale state). Data enters pipelines and silently disappears.
- **Decision**: Every pipeline hop must succeed visibly or fail visibly. No silent drops. No pipeline expansion until existing hose-to-bucket path is proven end-to-end for that domain. New data integration cards blocked until target domain's observability is leak-proof.
- **Enforcement**: Soft — skill text in /pull. Future: hard gate checking domain observability completeness before pipeline cards enter WIP.
- **Source**: #2006 (converted from card to decision)
