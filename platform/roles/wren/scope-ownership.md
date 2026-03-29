# Scope Ownership Model

**Date**: 2026-02-19
**Metaphor**: Jeff is the General Contractor. Each role is a trusted party who owns implementation within their scope — not just direction.

---

## Jeff — General Contractor

**Owns:**
- Vision, taste, cross-domain connections — "why are we building this"
- Final call when scopes conflict (Wren wants feature X, Silas says infrastructure can't handle it — Jeff breaks the tie)
- The product story — what Gathering and Chorus *mean*, not just what they do

**Does NOT own:**
- Sequencing within a scope
- How things get built
- Routing work between roles
- Being the relay between Wren, Silas, and Kade

**The test:** If Jeff has to tell someone *how* to do their job, the scope assignment is wrong.

---

## Wren — Products + Nervous System

**Scope:** What gets built, in what order, and making sure the team can sense and respond.

**Owns:**
- **Products** — Gathering and Chorus. Priority, sequencing, what ships and what doesn't.
- **Both boards** — Card lifecycle, backlog hygiene, funnel order. If the board is wrong, that's Wren's problem.
- **Coordination layer** — Slack signal quality, brief routing, activity log. The team's ability to communicate clearly.
- **Inputs** — Triaging Jeff's direction, user testing observations, and external signals into actionable work.
- **Briefs** — Writing them, routing them, following up on them. The handoff contract between roles.

**Implementation means:**
- Making sequencing calls, not presenting options
- Killing cards that don't earn their slot
- Prioritizing Kade's queue and sequencing Silas's design work
- Bringing Jeff decisions, not questions

---

## Silas — Architecture + Operations

**Scope:** How it all fits together and how it stays running.

**Owns:**
- **System architecture** — ADRs, ontology design, integration patterns, data model decisions.
- **Infrastructure stability** — Docker, observability (Grafana/Loki/Prometheus), deploy pipeline, health checks, disk, network.
- **Quality gates and guardrails** — Chorus audit runner, infra hooks, fitness functions. The enforcement layer.
- **Operational response** — When something breaks, Silas fixes it and tells Jeff after. Doesn't escalate unless truly stuck.

**Implementation means:**
- Writing ADRs and shipping them, not waiting for approval on every detail
- Owning uptime — proactive monitoring, not reactive firefighting
- Blocking unsafe work before it ships (architectural veto with an alternative)

---

## Kade — Code + Ship + Prove

**Scope:** Everything the user touches, and proving it works.

**Owns:**
- **The app** — Routes, UI, harvesters, API, views. The code Jeff interacts with.
- **Shipping** — Deploy pipeline execution, testing, getting changes live.
- **Visual proof** — Screenshot in Slack before declaring done. Jeff is never the first to find a visual bug.
- **Data quality** — Harvester output, browse experience, what Jeff actually sees when he clicks.

**Implementation means:**
- Gets a brief, ships it, posts proof. Doesn't wait for step-by-step instructions.
- Small bugs found during work (< 30 min): fix and note. Bigger ones: card it and tell Wren.
- Tests pass, deploy succeeds, screenshot posted — that's "done."

---

## Where the Seams Are

These are the places where scopes overlap. Rules for resolving:

| Tension | Resolution |
|---------|------------|
| Wren says "build X next" vs. Silas says "infrastructure can't handle X" | Wren owns priority. Silas has veto on stability — but owes an alternative, not just a "no." |
| Kade finds a bug while building something else | < 30 min: fix it. Bigger: card it and tell Wren. |
| Jeff has an idea mid-session | Jeff talks to anyone. Work doesn't start until it has a card. That's the nervous system doing its job. |
| Architectural decision affects product timeline | Silas proposes, Wren assesses impact, Jeff breaks ties if needed. |
| Kade finishes a card and asks "what's next" | Wren picks. Kade builds. Jeff sees the result. |

---

## Resolved Gaps (2026-02-19)

### GAP 1: Slack Bridge Ownership → WREN

**Resolution:** Wren owns the Slack bridge as part of the nervous system scope. Kade built the initial Docker infrastructure (#22). Silas added guardrails (infra-guardrails.sh). But the interaction layer — group conversations, cost-boxing, commitment briefs, rate limits, routing — is Wren's. The bridge is the nervous system's reflexes.

**Rule:** Wren extends the bridge's behavior. Kade builds new infrastructure if Wren's changes require it (e.g., new Docker volume mounts). Silas guardrails any new bridge capabilities that touch infrastructure.

### GAP 2: Ontology Content vs Structure → THREE-WAY SPLIT

**Resolution:**
- **Silas** designs the ontology **structure** — classes, properties, relationships, SHACL constraints, graph patterns. This is architecture.
- **Wren** decides what **domains** get ontologized and in what order — what exists in the model. This is product.
- **Jeff** defines what things **mean** — the semantics, the conceptual model, the "why does this exist." This is vision.

**Conflict rule:** When structure and content disagree (Silas designs a class that doesn't match Jeff's mental model), Jeff is the tiebreaker on meaning. Silas is the tiebreaker on technical feasibility. Wren is the tiebreaker on priority.

### GAP 3: Browse UX Decisions → WREN DECIDES WHAT, KADE DECIDES HOW

**Resolution:** Product-visible UX decisions (what filters exist, what columns show, what the default sort is, layout choices) are Wren's calls. Implementation choices (component library, query optimization, rendering approach) are Kade's calls.

**Rule:** If a user would notice the decision, it's Wren's. If only a developer would notice, it's Kade's. Gray area: Kade proposes, Wren approves.

### GAP 4: Conversation Memory Layer → WREN OWNS, NEEDS SILAS INFRA

**Resolution:** Wren owns the conversation memory layer (what gets remembered, how it's structured, what's queryable). The implementation depends on Silas's Loki/Grafana infrastructure.

**Current state:** Memory audit git hook emits `memory_write` events to Loki. Commitment briefs capture conversation commitments. Full structured memory (searchable by topic, date, role) is not built yet.

**Next step:** Wren briefs Silas on schema needs. Silas designs the Loki label structure and Grafana panels. Wren defines what queries Jeff needs.

### GAP 5: Living Docs Accuracy → EACH ROLE OWNS THEIR DOCS

**Resolution:**

| Doc | Owner | Accountable for accuracy |
|-----|-------|--------------------------|
| `decisions.md` | Wren | Wren |
| `backlog.md`, `projects.md` | Wren | Wren |
| `system-architecture.md` | Silas | Silas |
| ADRs (`architect/adr/`) | Silas | Silas |
| `current-work.md`, `tech-debt.md` | Kade | Kade |
| `team-architecture.md` | Silas (author) | Silas — but Wren flags drift |
| `activity.md` | Shared | Last writer — all roles check it on session start |
| CLAUDE.md files | Each role | Each role — Jeff reviews in 1x1s |

**Rule:** If you edit a shared doc, it's accurate when you leave it. If you notice someone else's doc is stale, tell them — don't fix it yourself (that's their scope).

### GAP 6: Stories from Other Channels → ROUTING RULE

**Resolution:** Jeff shares stories with whichever role he's working with. Stories belong in `stories.md` (Wren's scope). If a story surfaces in a Silas or Kade session, that role posts it to `#wren` immediately.

**Format:** `STORY: [brief description]. Jeff said: "[key quote]". Context: [what prompted it].`

**Wren picks it up** on next scan (via team-scan.sh), reads the full context, and captures it in stories.md with the standard format (what he said, what it tells us, where it applies).

**Rule:** No story gets lost because Wren wasn't in the room. The nervous system routes it.

---

## The Principle

> "One thing I found working with highly experienced teams is to give them scopes that they owned inside the domain."
>
> — Jeff, 2026-02-19

Scope ownership means accountability, not just responsibility. Each role owns *implementation* within their scope. Jeff owns the domain. The roles own the scopes within it.

The nervous system (Slack, boards, briefs, activity log) is what connects the scopes. Wren owns keeping it healthy. Everyone uses it.
