# Chorus Domain Glossary — DRAFT

**Status:** `confidence:initial` — pre-Jeff-signoff draft from Explore research (#3109). Every entry's one-sentence definition needs Jeff's sign-off before this loses the `:initial` tag. Conflicts and gaps surfaced at the bottom — those are decisions, not glossary entries.

**Scope:** Chorus product domains only. Gathering-only domains out of scope.

**Sources:** chorus-product-tree.md (2026-05-14/16), athena-subproduct-design.html (2026-05-15), chorus-service-design.html (2026-04-11/18), chorus-pulse-tobe.html, ADRs 024 / 028 / 031 / 032 / 095, all role CLAUDE.md files, TEAM_PROTOCOL.md, live Athena subdomain registry.

**Open meta-decision (Jeff, 2026-05-28):** Werk is the team's universal protocol for moving any unit of work through state across all five value-streams (Shaping / Designing / Directing / Building / Proving), not a Building-step subproduct. Glossary entries below need to be reread under that reframe. The Werk entry already reflects it; Spine and most others probably do too. Flagged at the bottom.

---

## Athena

- **One sentence:** The structural and query layer that holds the team's product model (products, domains, services, roles, records) and makes it queryable via API.
- **Owner:** Wren
- **Adjacent terms it gets confused with:**
  - Knowledge graph — Athena is the schema + query surface, not the knowledge store (that's Loom)
  - Ontology — Athena uses the ontology; Athena is the instance layer
  - Catalog — Athena feeds the catalog but is broader (typed API over RDF + SHACL)
- **Canonical sub-terms:** Product, Domain, Service, Role, ValueStreamStep, Record (Actor, Scenario, Decision, Endpoint, Page, Pipeline, LogStream, Alert, Test, Component, Glossary, Reference, Release, PriorArt, ConsumerContract).
- **Citations:** athena-subproduct-design.html; ADR-025; ADR-028.
- **Conflicts surfaced:** Recursive Product collapse (SubProduct → Product/hasChild) is in-flight intentional refactor. Service nodes (cards-service, gates-service) currently mis-typed as SubDomain — API exposes 3 of 29 node types until type-migration completes.

---

## Borg

- **One sentence:** The reflection product — the system sees itself via heralds that discover toolchain, infrastructure, commits, deploys, alerts.
- **Owner:** Silas
- **Adjacent terms it gets confused with:**
  - Observability — Borg *structures* what to observe; observability is the cross-cutting capability *within* Borg
  - Monitoring — Borg discovers + models; monitoring is the running watch
  - Infrastructure — Borg models it; infrastructure is the hosting layer
- **Canonical sub-terms:** Herald (discover-toolchain, discover-services, discover-storage, discover-network, discover-commit-pipeline, discover-deploy-paths, discover-alerts), Engine, Environment, Resource. Sub-domains: Toolchain, Infrastructure, Commits, Deploys, Alerts & Monitors, Observability (cross-cutting), Security (cross-cutting).
- **Citations:** borg-service-design.md (2026-04-15).
- **Conflicts surfaced:** Three-tier Engine/Environment/Resource model is new + not fully wired into the graph; herald implementations mid-flight.

---

## Clearing

- **One sentence:** Real-time browser surface aggregating role state, messages, session tails, alerts, and multi-party chat into one glance.
- **Owner:** Wren (with Silas on supporting domains)
- **Adjacent terms it gets confused with:**
  - Board — Board is the kanban (cards); Clearing is the observational surface (what's happening right now)
  - Chat tool — Clearing includes chat but is primarily the team's ops dashboard
  - Dashboard — Clearing is the team-level dashboard; Grafana is the ops-level one
- **Canonical sub-terms:** Role Tiles, Message Stream, Session Tailer, Alerts Panel, Multi-Party Chat, Remote Access. Backed by six supporting domains: Coordination, Nudge, Observability, Awareness, Spine, Loom.
- **Citations:** clearing-service-design.html (2026-04-06).
- **Conflicts surfaced:** None unresolved.

---

## Convergence

- **One sentence:** **Undefined in current docs.** Named as subproduct under Designing step; no design doc, no owner, no sub-term list.
- **Owner:** TBD — needs Jeff sign-off
- **Possible interpretations:** the ICD layer (how data integrates) / the Bridge layer (message relay) / the integration *pattern* collection.
- **Inferred sub-terms:** Bridge, ICDs, NiFi engine (per memory note: convergence = ICDs + NiFi engine, lives in gathering, must move to chorus).
- **Citations:** chorus-product-tree.md (lists under DESIGNING step, no further detail).
- **Conflicts surfaced:** Gap requiring Jeff sign-off. Memory says convergence = data integration (ICDs + NiFi); product tree doesn't confirm.

---

## Loom

- **One sentence:** The shared knowledge and coordination layer — principles, practices, policies, skills, decisions, briefs, RCAs, stories, patterns.
- **Owner:** Wren
- **Adjacent terms it gets confused with:**
  - Principles — Principles live in Loom but are one entity type among many
  - Library — A library is a document collection; Loom is the team coordination substrate (who knows what, how decisions flow)
  - Chorus knowledge — Loom IS chorus knowledge; chorus is the coordination protocol, Loom is the knowledge layer inside it
- **Canonical sub-terms:** Principle, Practice, Policy, Skill (callable capability: `/demo`, `/pull`, `/card`), Verb (zero-dep binary: werk-pull, werk-commit, etc.), Binary, MCP tool, Decision (DEC or ADR), Brief, Story, RCA, Pattern, Interaction-pattern.
- **Citations:** loom-subproduct-design.html (reference impl for ADR-028); ADR-028; Wren CLAUDE.md.
- **Conflicts surfaced:** Stories are collected but not formally plumbed as a searchable subdomain (live in shared memory; lack drift test + API surface). The skill vs verb vs binary vs MCP-tool distinction is informally understood but not formally pinned — biggest definitional gap inside Loom.

---

## Werk

- **One sentence:** The team's universal protocol for moving any unit of work through state across all five value-streams (Shaping, Designing, Directing, Building, Proving) — verbs, traces, witnesses, and acceptance.
- **Owner:** Kade (with Silas on orchestration/gates, Wren on card coordination)
- **Adjacent terms it gets confused with:**
  - Workspace — Werk is not a directory; it's the whole protocol
  - Worktree — A worktree is one Werk concept (ephemeral per-card directory); Werk is broader
  - Build pipeline — Building is one value-stream Werk covers; Werk extends to all five
- **Canonical sub-terms:** Worktree (ephemeral, `~/.chorus/werk/<role>-<card>/`). Verb (atomic zero-dep Rust binary, ADR-032). Building-step verbs today: pull, commit, build, deploy, verify, accept. Orchestrator (demo, acp — composite verbs invoking siblings). Trace (one trace_id across the verb chain, persisted `/tmp/<card>-trace`). Witness (best-effort JSONL log to `ops/logs/werk-<verb>.jsonl`).
- **Open extension (Jeff, 2026-05-28):** Werk verbs are needed for Shaping (pull-an-idea, refine, accept-as-shape), Designing (pull-a-question, propose, decide), Directing (pull-into-flight, prioritize, route), Proving (the demo+acp pair is already the Proving-step protocol completion). Today only Building has a verb set.
- **Citations:** ADR-032 (verb contract v1); version-control-service-design.html.
- **Conflicts surfaced:** Was placed under Building in product-tree.md; the 2026-05-28 reframe puts it as a cross-cutting protocol layer. Tree needs updating. No service-design.html — that's intentional under the reframe (protocols get specifications, not service designs).

---

## Spine

- **One sentence:** The universal record-face of every werk verb and integration action — typed events with trace_id, card_id, role, metadata, queryable and replay-able.
- **Owner:** Silas
- **Adjacent terms it gets confused with:**
  - Log — Spine events are typed and queryable, not unstructured text
  - Event bus — Spine is the chorus-specific event bus
  - Message bus — Spine records state-transitions, not just message-relay
- **Canonical sub-terms:** Event (typed record: name, role, card_id, timestamp, trace_id, metadata). Event classes: card.pulled, card.accepted, service.deployed, interaction.pattern.detected, binary.deployed, demo.show.completed, etc. Storage: Loki via Promtail. Query: Loki API + `chorus_logs_for_card` / `chorus_logs_for_trace` MCP tools.
- **Citations:** ADR-024 (common message envelope); chorus-service-design.html.
- **Conflicts surfaced:** No spine-service-design.html. The 2026-05-28 reframe (with Jeff): Spine and Werk are two faces of one substrate — Werk = action-face, Spine = record-face. They converge by construction (verbs emit, trace_id threads). Implication: one substrate doc covering both, not two parallel designs. MCP-on-spine becomes the natural completion — every action surface emits, period.

---

## Nudge

- **One sentence:** Role-to-role message delivery — immediate via osascript injection to terminal + persisted via messaging API.
- **Owner:** Wren / Silas (shared)
- **Adjacent terms it gets confused with:**
  - Ping — Nudge requires action (DEC-1571: respond within 60s)
  - Alert — Nudge is role-to-role; alerts are system-to-role
  - Notification — Nudge targets the role's active terminal; notifications are broader
- **Canonical sub-terms:** Nudge message, two-path delivery (osascript + API persist), messaging API at `localhost:3475`, `chorus_nudge_message` MCP tool.
- **Citations:** DEC-107 (locked two-path design); DEC-1571 (Attention Contract); clearing-service-design.html.
- **Conflicts surfaced:** DEC-107 names two paths as locked but doesn't fully explain why both are necessary. Worth a one-line rationale addendum.

---

## Awareness

- **One sentence:** Session tailing and team-scan — what each role is emitting, aggregated per-role and per-session for cross-role observation.
- **Owner:** Silas
- **Adjacent terms it gets confused with:**
  - Observability — Awareness is team-level (what roles are doing); observability is system-level (what infra is doing)
  - Logging — Awareness consumes logs but is role-scoped filtering + digest, not raw aggregation
- **Canonical sub-terms:** Session tail, Team-scan digest, Observer digest, `observations.jsonl` per-role.
- **Citations:** clearing-service-design.html (named as supporting domain).
- **Conflicts surfaced:** No dedicated service design. Concept live, formal contract scattered.

---

## Observability (cross-cutting)

- **One sentence:** The lens that makes the system see itself — metrics, logs, dashboards, health checks, instrumentation across all layers.
- **Owner:** Silas
- **Adjacent terms it gets confused with:**
  - Monitoring — Observability is the capability; monitoring is the running watch
  - Debugging — Observability is always-on baseline; debugging is reactive
  - Borg — Borg reflects on the system; observability is how reflection happens
- **Canonical sub-terms:** Metrics pipeline (Prometheus, 15s/15d), Logs pipeline (Loki + Promtail, 7d, JSON), Health pipeline (deep-health.sh every 5min), Instrumentation (hooks pulse log, spine events), Dashboards (13 in Grafana).
- **Citations:** borg-service-design.md (definitive cross-cutting section).
- **Conflicts surfaced:** None.

---

## Security (cross-cutting)

- **One sentence:** Protection against external and internal threats — auth, permission models, write-scrubber, session boundaries, CVE tracking.
- **Owner:** Silas
- **Adjacent terms it gets confused with:**
  - Compliance — Security is defensive posture; compliance is external requirements
  - Access control — AC is part of security; security is broader
  - Privacy — Security includes privacy; security is broader
- **Canonical sub-terms:** Pre-commit hooks (write-scrubber), session boundaries, permission model (AllowList in settings.json), CVE tracking, audit trail (activity.md, spine events, git history).
- **Citations:** borg-service-design.md (cross-cutting section); CLAUDE.md Data Safety sections.
- **Conflicts surfaced:** No security-service-design.html. Implementation live; formal threat model + per-layer risk assessment fragmentary.

---

## Coordination

- **One sentence:** Role state and card transitions — who's building what, who's blocked, what card is in flight.
- **Owner:** Wren
- **Adjacent terms it gets confused with:**
  - Board — Board is the tool; Coordination is the state machine driving it
  - Sync — Coordination is structured handoff, not just sync
- **Canonical sub-terms:** Andon state (building, blocked, waiting, observing, idle), Role state file, Card status (Now, Next, Later, WIP, Blocked, Done, Won't Do).
- **Citations:** clearing-service-design.html (named as supporting domain); Wren CLAUDE.md.
- **Conflicts surfaced:** Not in product-tree.md as a Domain. May be a capability inside Directing rather than a named domain. Decision needed.

---

## ValueStreamStep (named but unclear shape)

- **One sentence:** The five steps work flows through: Shaping → Designing → Directing → Building → Proving.
- **Owner:** Wren
- **Adjacent terms it gets confused with:** These are not Gathering's garden-lifecycle steps (Sowing/Growing/Tending/Harvesting/Practicing/Reflecting — those belong to Gathering).
- **Canonical sub-terms:** Each step is a Vertebra node in the graph (urn:chorus:instances).
- **Conflicts surfaced:** No single domain named "ValueStream" or "Verse"; the axis is named only by individual steps. Is the axis itself a Domain? Or a property (atStep: Product → Step)? Decision needed.

---

# Conflicts requiring resolution

1. **Recursive Product collapse** — in-flight refactor; TO-BE is canonical; AS-IS transitional.
2. **Service mis-typing** — Service nodes typed SubDomain in graph; type-migration pending.
3. **Werk under Building** — placed there in product-tree.md; 2026-05-28 reframe puts it as cross-cutting protocol; tree needs updating.
4. **Spine ↔ Werk convergence** — two faces of one substrate (action / record); should not be modeled separately.
5. **MCP-on-spine** — every action surface should emit; not yet enforced at the gateway. ADR-031 covers naming, not spine emission. ADR-036 (sibling) or ADR-031 amendment needed.
6. **ValueStream axis vs step** — axis named only by its five steps. Domain or property?

# Gaps requiring Jeff sign-off

1. **Convergence** — undefined in docs; needs scope + owner + sub-term list. Memory says ICDs + NiFi; product tree doesn't confirm.
2. **Awareness** — supporting-domain reference in Clearing spec; no standalone spec.
3. **Spine** — no service-design.html. Under the converge-with-Werk reframe, may not need one as a peer; needs the joint substrate doc instead.
4. **Werk** — taxonomy resolution under cross-cutting-protocol reframe; tree update; whether it gets a specification (not service design).
5. **Coordination** — domain vs capability inside Directing?
6. **Security** — cross-cutting capability with fragmentary spec; security-service-design.html or integrated treatment?
7. **Nudge two-path rationale** — DEC-107 locked but unexplained.
8. **Stories subdomain** — gets the full ADR-028 completeness treatment, or stays in shared memory?
9. **ICD-provider coverage** — ADR-095 references provider sections; audit needed of which domains lack providers.
10. **Werk verb sets for Shaping / Designing / Directing / Proving** — Building has six verbs; the other four steps have zero. Open arc, not glossary work, but the reframe surfaces them.

---

**Next move:** Jeff edits each entry's one-sentence definition; we resolve the conflicts + gaps as a working session; glossary loses `confidence:initial` tag; cites from chorus-product-tree.md + chorus-pulse-tobe.html. Then OWL/BDD/actor session can sit on a fixed term substrate.
